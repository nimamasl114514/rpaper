//! Wallpaper Engine .pkg 文件解析
//!
//! PKG 是 WE 的二进制打包格式（非 ZIP）。
//! 结构:
//!   Header  : 8 字节 magic ("PKGV0001" / "PKGV0005" / "PKGM0019") + uint32 version + uint32 file_count
//!   Index   : file_count 个条目 (path_length + path + offset + compressed_size + uncompressed_size [+ 16B hash for v5+])
//!   Data    : 各文件 LZ4 块压缩数据，offset 相对数据段起点
//!
//! 加载流程拆为两阶段，避免主线程被大包解压阻塞:
//!   1. probe_pkg() — 同步快速打开 + 解析头 + 读 project.json + 定位视频/音频条目（毫秒级）
//!   2. PkgProbe::extract() — 实际解压视频到临时文件（耗时操作，应在后台线程执行）
//!
//! 仅对 video 类型壁纸做完整支持 (抽 mp4 + audio)，scene/web/application 类型给出友好提示。

use serde::Deserialize;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// PKG 解析错误 — 按阶段分类，便于 UI 给出针对性提示
#[derive(Debug)]
pub enum PkgError {
    /// 不是 .pkg 文件（magic 不匹配）
    NotPkg,
    /// 头部/索引损坏（file_count 异常、path 过长、条目缺失等）
    CorruptHeader(String),
    /// 缺 project.json
    MissingProject,
    /// project.json 解析失败
    InvalidProject(String),
    /// WE 不支持的壁纸类型（scene/web/application）
    UnsupportedType(String),
    /// 缺视频文件
    MissingVideo,
    /// LZ4 解压失败（数据损坏）
    Lz4Corrupt(String),
    /// 其他 IO 错误
    Io(String),
}

impl std::fmt::Display for PkgError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PkgError::NotPkg => write!(f, "不是 Wallpaper Engine .pkg 文件 (magic 不匹配)"),
            PkgError::CorruptHeader(s) => write!(f, ".pkg 头部损坏: {s}"),
            PkgError::MissingProject => write!(f, ".pkg 缺少 project.json"),
            PkgError::InvalidProject(s) => write!(f, "解析 project.json 失败: {s}"),
            PkgError::UnsupportedType(t) => write!(
                f,
                "不支持 Wallpaper Engine 「{t}」类型壁纸\n\
                 rpaper 仅支持 WE 的 video 类型 .pkg，scene/web/application 类型需要完整 Wallpaper Engine 渲染器\n\
                 请用 Wallpaper Engine 编辑器将该壁纸导出为视频文件后导入"
            ),
            PkgError::MissingVideo => write!(f, ".pkg 没有视频文件"),
            PkgError::Lz4Corrupt(s) => write!(f, "LZ4 解压失败: {s}"),
            PkgError::Io(s) => write!(f, "{s}"),
        }
    }
}

impl From<io::Error> for PkgError {
    fn from(e: io::Error) -> Self {
        PkgError::Io(e.to_string())
    }
}

/// project.json 的最小子集
#[derive(Debug, Deserialize)]
struct ProjectJson {
    #[serde(rename = "type")]
    wallpaper_type: String,
    /// video 类型壁纸的源文件路径（相对包内）
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PkgEntry {
    pub path: String,
    pub offset: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
}

#[derive(Debug)]
pub struct PkgFile {
    #[allow(dead_code)]
    pub magic: String,
    #[allow(dead_code)]
    pub version: u32,
    pub entries: Vec<PkgEntry>,
    /// 数据段在文件中的绝对偏移
    data_offset: u64,
    file: std::fs::File,
}

impl PkgFile {
    pub fn open(path: &Path) -> Result<Self, PkgError> {
        let mut file = std::fs::File::open(path)?;

        // 8 字节 ASCII magic
        let mut magic_buf = [0u8; 8];
        file.read_exact(&mut magic_buf)?;
        let magic = String::from_utf8_lossy(&magic_buf).to_string();
        if !magic.starts_with("PKG") {
            return Err(PkgError::NotPkg);
        }

        let mut buf4 = [0u8; 4];
        file.read_exact(&mut buf4)?;
        let version = u32::from_le_bytes(buf4);
        file.read_exact(&mut buf4)?;
        let file_count = u32::from_le_bytes(buf4) as usize;

        // 限制上界，防止畸形文件分配过大内存
        if file_count > 65536 {
            return Err(PkgError::CorruptHeader(format!(
                "file_count 异常: {file_count}"
            )));
        }

        let mut entries = Vec::with_capacity(file_count);
        for _ in 0..file_count {
            file.read_exact(&mut buf4)?;
            let path_len = u32::from_le_bytes(buf4) as usize;
            if path_len > 4096 {
                return Err(PkgError::CorruptHeader("path 过长".into()));
            }
            let mut path_bytes = vec![0u8; path_len];
            file.read_exact(&mut path_bytes)?;
            let path = String::from_utf8_lossy(&path_bytes).to_string();

            file.read_exact(&mut buf4)?;
            let offset = u32::from_le_bytes(buf4);
            file.read_exact(&mut buf4)?;
            let compressed_size = u32::from_le_bytes(buf4);
            file.read_exact(&mut buf4)?;
            let uncompressed_size = u32::from_le_bytes(buf4);

            // PKGV0005+ 每个条目末尾带 16 字节 MD5 hash
            if version >= 5 {
                let mut hash = [0u8; 16];
                file.read_exact(&mut hash)?;
            }

            entries.push(PkgEntry {
                path,
                offset,
                compressed_size,
                uncompressed_size,
            });
        }

        let data_offset = file.stream_position()?;

        Ok(Self {
            magic,
            version,
            entries,
            data_offset,
            file,
        })
    }

    /// 解压指定条目
    pub fn read_entry(&mut self, entry: &PkgEntry) -> Result<Vec<u8>, PkgError> {
        self.file
            .seek(SeekFrom::Start(self.data_offset + entry.offset as u64))?;
        let mut compressed = vec![0u8; entry.compressed_size as usize];
        self.file.read_exact(&mut compressed)?;
        lz4_flex::decompress(&compressed, entry.uncompressed_size as usize)
            .map_err(|e| PkgError::Lz4Corrupt(e.to_string()))
    }

    /// 大小写+路径分隔符不敏感地查找条目
    pub fn find(&self, name: &str) -> Option<&PkgEntry> {
        let target = name.to_lowercase().replace('\\', "/");
        self.entries.iter().find(|e| {
            let p = e.path.to_lowercase().replace('\\', "/");
            p == target || p.ends_with(&format!("/{target}"))
        })
    }

    /// 列出所有条目路径（调试用）
    #[allow(dead_code)]
    pub fn list(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.path.as_str()).collect()
    }
}

/// probe 阶段产物 — 头部 + 索引 + project.json + 视频/音频条目定位完成
/// extract 阶段在后台线程执行实际解压
pub struct PkgProbe {
    pkg: PkgFile,
    title: String,
    video_entry: PkgEntry,
    audio: Option<(PkgEntry, String)>,
}

impl PkgProbe {
    #[allow(dead_code)]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// 执行实际解压 — 把视频写到临时文件，音频读进内存
    /// 耗时操作，应在后台线程调用
    pub fn extract(mut self) -> Result<PkgVideo, PkgError> {
        let video_ext = Path::new(&self.video_entry.path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4")
            .to_lowercase();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp_path = std::env::temp_dir().join(format!(
            "rpaper_pkg_{}_{}.{}",
            std::process::id(),
            nanos,
            video_ext
        ));

        let video_data = self.pkg.read_entry(&self.video_entry)?;
        std::fs::write(&temp_path, &video_data)
            .map_err(|e| PkgError::Io(format!("写临时文件失败: {e}")))?;
        // 显式释放 video_data 内存，避免后续 audio 读取时双份占用
        drop(video_data);

        let (audio_data, audio_ext) = if let Some((entry, ext)) = self.audio.take() {
            match self.pkg.read_entry(&entry) {
                Ok(data) => (Some(data), ext),
                // 音频解压失败不致命，视频仍可播放
                Err(_) => (None, String::new()),
            }
        } else {
            (None, String::new())
        };

        Ok(PkgVideo {
            video_temp_path: temp_path,
            audio_data,
            audio_ext,
            title: self.title,
        })
    }
}

/// probe 阶段 — 同步快速打开 + 解析头 + 读 project.json + 定位视频/音频条目
/// 毫秒级，可在主线程安全调用
pub fn probe_pkg(path: &Path) -> Result<PkgProbe, PkgError> {
    let mut pkg = PkgFile::open(path)?;

    // 解析 project.json
    let project: ProjectJson = {
        let entry = pkg
            .find("project.json")
            .ok_or(PkgError::MissingProject)?
            .clone();
        let data = pkg.read_entry(&entry)?;
        serde_json::from_slice(&data)
            .map_err(|e| PkgError::InvalidProject(e.to_string()))?
    };

    let title = project.title.unwrap_or_else(|| "Wallpaper Engine 视频".into());

    if project.wallpaper_type != "video" {
        return Err(PkgError::UnsupportedType(project.wallpaper_type));
    }

    // 定位视频文件: 优先用 project.file 字段，否则按扩展名找
    let video_entry = if let Some(file_field) = &project.file {
        pkg.find(file_field)
            .ok_or_else(|| {
                PkgError::CorruptHeader(format!(
                    "project.json 指定的视频文件 '{file_field}' 在 .pkg 中不存在"
                ))
            })?
            .clone()
    } else {
        pkg.entries
            .iter()
            .find(|e| {
                let p = e.path.to_lowercase();
                p.ends_with(".mp4")
                    || p.ends_with(".webm")
                    || p.ends_with(".mov")
                    || p.ends_with(".mkv")
                    || p.ends_with(".avi")
            })
            .ok_or(PkgError::MissingVideo)?
            .clone()
    };

    // 定位音频（按扩展名查找）
    let audio = pkg
        .entries
        .iter()
        .find(|e| {
            let p = e.path.to_lowercase();
            p.ends_with(".mp3")
                || p.ends_with(".ogg")
                || p.ends_with(".wav")
                || p.ends_with(".flac")
                || p.ends_with(".m4a")
                || p.ends_with(".aac")
        })
        .map(|e| {
            let ext = Path::new(&e.path)
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("mp3")
                .to_lowercase();
            (e.clone(), ext)
        });

    Ok(PkgProbe {
        pkg,
        title,
        video_entry,
        audio,
    })
}

/// 兼容接口 — probe + extract 顺序执行
/// 用于测试或不需要后台解压的场景；生产路径应分两阶段调用以避免主线程阻塞
#[allow(dead_code)]
pub fn load_video_pkg(path: &Path) -> Result<PkgVideo, String> {
    probe_pkg(path)
        .and_then(|p| p.extract())
        .map_err(|e| e.to_string())
}

/// PKG 解包结果 — 仅 video 类型有有效字段
pub struct PkgVideo {
    /// 解压出的视频临时文件路径（Drop 时自动清理）
    pub video_temp_path: PathBuf,
    /// 背景音乐原始数据（如果有）
    pub audio_data: Option<Vec<u8>>,
    /// 背景音乐文件扩展名
    pub audio_ext: String,
    #[allow(dead_code)]
    pub title: String,
}

impl Drop for PkgVideo {
    fn drop(&mut self) {
        // Windows 下文件可能正被占用，忽略删除失败
        let _ = std::fs::remove_file(&self.video_temp_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 构造一个 PKG 字节流
    /// `version`: 1 (无 hash) 或 5 (16字节 hash)
    /// `entries`: (path, uncompressed_data) 列表
    fn build_pkg(version: u32, entries: &[(&str, &[u8])]) -> Vec<u8> {
        let magic = if version >= 5 {
            b"PKGV0005"
        } else {
            b"PKGV0001"
        };

        // 先压缩所有数据，计算 offset
        let mut compressed_blocks: Vec<Vec<u8>> = Vec::with_capacity(entries.len());
        let mut offsets: Vec<(u32, u32, u32)> = Vec::with_capacity(entries.len()); // offset, compressed, uncompressed
        let mut current_offset: u32 = 0;
        for (_, data) in entries {
            let compressed = lz4_flex::compress(data);
            offsets.push((
                current_offset,
                compressed.len() as u32,
                data.len() as u32,
            ));
            current_offset += compressed.len() as u32;
            compressed_blocks.push(compressed);
        }

        let mut buf: Vec<u8> = Vec::new();
        // Header
        buf.extend_from_slice(magic);
        buf.extend_from_slice(&version.to_le_bytes());
        buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());

        // Index
        for (i, (path, _)) in entries.iter().enumerate() {
            let path_bytes = path.as_bytes();
            buf.extend_from_slice(&(path_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(path_bytes);
            let (off, csize, usize_) = offsets[i];
            buf.extend_from_slice(&off.to_le_bytes());
            buf.extend_from_slice(&csize.to_le_bytes());
            buf.extend_from_slice(&usize_.to_le_bytes());
            // PKGV0005+ 每条目末尾 16 字节 hash (测试用全 0)
            if version >= 5 {
                buf.extend_from_slice(&[0u8; 16]);
            }
        }

        // Data section
        for compressed in compressed_blocks {
            buf.extend_from_slice(&compressed);
        }

        buf
    }

    fn write_temp_pkg(data: &[u8]) -> std::io::Result<std::path::PathBuf> {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!(
            "rpaper_pkg_test_{}_{}.pkg",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        Ok(tmp)
    }

    #[test]
    fn parse_pkgv0001_basic() {
        let entries: Vec<(&str, &[u8])> = vec![
            ("project.json", b"{\"type\":\"video\",\"file\":\"video.mp4\",\"title\":\"test\"}"),
            ("video.mp4", b"FAKE_MP4_DATA_BYTES"),
            ("audio.mp3", b"FAKE_MP3_AUDIO"),
        ];
        let pkg_bytes = build_pkg(1, &entries);
        let tmp = write_temp_pkg(&pkg_bytes).unwrap();
        let mut pkg = PkgFile::open(&tmp).unwrap();
        assert_eq!(pkg.version, 1);
        assert_eq!(pkg.entries.len(), 3);
        assert_eq!(pkg.entries[0].path, "project.json");

        // 读取 project.json
        let entry = pkg.find("project.json").unwrap().clone();
        let data = pkg.read_entry(&entry).unwrap();
        assert!(data.starts_with(b"{\"type\":\"video\""));

        // 读取 video.mp4
        let entry = pkg.find("video.mp4").unwrap().clone();
        let data = pkg.read_entry(&entry).unwrap();
        assert_eq!(data, b"FAKE_MP4_DATA_BYTES");

        // 读取 audio.mp3
        let entry = pkg.find("audio.mp3").unwrap().clone();
        let data = pkg.read_entry(&entry).unwrap();
        assert_eq!(data, b"FAKE_MP3_AUDIO");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn parse_pkgv0005_with_hash() {
        let entries: Vec<(&str, &[u8])> = vec![
            ("project.json", b"{\"type\":\"video\"}"),
            ("video.mp4", b"X"),
        ];
        let pkg_bytes = build_pkg(5, &entries);
        let tmp = write_temp_pkg(&pkg_bytes).unwrap();
        let mut pkg = PkgFile::open(&tmp).unwrap();
        assert_eq!(pkg.version, 5);
        assert_eq!(pkg.entries.len(), 2);

        let entry = pkg.find("video.mp4").unwrap().clone();
        let data = pkg.read_entry(&entry).unwrap();
        assert_eq!(data, b"X");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn reject_non_pkg_file() {
        let tmp = write_temp_pkg(b"NOT_A_PKG_FILE").unwrap();
        let result = PkgFile::open(&tmp);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PkgError::NotPkg
        ));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn load_video_pkg_works() {
        let entries: Vec<(&str, &[u8])> = vec![
            ("project.json", b"{\"type\":\"video\",\"file\":\"video.mp4\",\"title\":\"My WE Wallpaper\"}"),
            ("video.mp4", b"FAKE_MP4_DATA"),
            ("audio.mp3", b"FAKE_AUDIO"),
        ];
        let pkg_bytes = build_pkg(1, &entries);
        let tmp = write_temp_pkg(&pkg_bytes).unwrap();

        let result = load_video_pkg(&tmp);
        assert!(result.is_ok(), "load_video_pkg failed: {:?}", result.err());
        let pkg_video = result.unwrap();
        assert!(pkg_video.video_temp_path.exists());
        assert!(pkg_video.audio_data.is_some());
        assert_eq!(pkg_video.audio_ext, "mp3");
        assert_eq!(pkg_video.title, "My WE Wallpaper");

        // 临时文件应在 Drop 时被清理
        let path_clone = pkg_video.video_temp_path.clone();
        drop(pkg_video);
        assert!(!path_clone.exists(), "临时文件未被清理");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn load_video_pkg_rejects_scene_type() {
        let entries: Vec<(&str, &[u8])> = vec![("project.json", b"{\"type\":\"scene\"}")];
        let pkg_bytes = build_pkg(1, &entries);
        let tmp = write_temp_pkg(&pkg_bytes).unwrap();

        let result = load_video_pkg(&tmp);
        let err = result.err().expect("应当返回错误");
        assert!(err.contains("scene"), "错误信息应包含类型名: {err}");

        let _ = std::fs::remove_file(&tmp);
    }

    /// 验证 probe + extract 两阶段拆分正确
    #[test]
    fn probe_then_extract_works() {
        let entries: Vec<(&str, &[u8])> = vec![
            ("project.json", b"{\"type\":\"video\",\"file\":\"v.mp4\",\"title\":\"Probe Test\"}"),
            ("v.mp4", b"VIDEO_DATA_HERE"),
            ("bg.ogg", b"AUDIO_DATA"),
        ];
        let pkg_bytes = build_pkg(1, &entries);
        let tmp = write_temp_pkg(&pkg_bytes).unwrap();

        // probe 阶段 — 不解压视频，仅定位
        let probe = probe_pkg(&tmp).expect("probe 应成功");
        assert_eq!(probe.title(), "Probe Test");

        // extract 阶段 — 实际解压
        let pkg_video = probe.extract().expect("extract 应成功");
        assert!(pkg_video.video_temp_path.exists());
        assert_eq!(pkg_video.audio_ext, "ogg");
        assert!(pkg_video.audio_data.is_some());

        let _ = std::fs::remove_file(&tmp);
    }

    /// probe 阶段就应识别 WE 不支持的类型，避免启动后台线程后才报错
    #[test]
    fn probe_rejects_unsupported_type_early() {
        let entries: Vec<(&str, &[u8])> =
            vec![("project.json", b"{\"type\":\"application\"}")];
        let pkg_bytes = build_pkg(1, &entries);
        let tmp = write_temp_pkg(&pkg_bytes).unwrap();

        let result = probe_pkg(&tmp);
        let err = result.err().expect("probe 应返回错误");
        assert!(
            matches!(err, PkgError::UnsupportedType(ref t) if t == "application"),
            "应当是 UnsupportedType(application), 实际: {err:?}"
        );

        let _ = std::fs::remove_file(&tmp);
    }

    /// probe 阶段就应识别缺 project.json，避免无效解压
    #[test]
    fn probe_rejects_missing_project() {
        let entries: Vec<(&str, &[u8])> = vec![("video.mp4", b"X")];
        let pkg_bytes = build_pkg(1, &entries);
        let tmp = write_temp_pkg(&pkg_bytes).unwrap();

        let result = probe_pkg(&tmp);
        assert!(matches!(result, Err(PkgError::MissingProject)));

        let _ = std::fs::remove_file(&tmp);
    }
}
