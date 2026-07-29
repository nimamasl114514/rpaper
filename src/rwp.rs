//! 壁纸包格式 (.rwp) — ZIP 压缩包，内含 manifest.json + 资源文件
//!
//! 结构:
//!   example.rwp (ZIP)
//!   ├── manifest.json   { name, type, author, description, audio }
//!   ├── shader.wgsl     (shader 类型)
//!   ├── image.png       (image 类型)
//!   ├── video.mp4       (video 类型)
//!   └── audio.mp3       (可选，背景音乐)

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    #[serde(rename = "type")]
    pub wallpaper_type: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub audio: Option<String>,
    /// 自定义着色器的 uniform 参数（JSON 对象，转成 f32 数组）
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug)]
pub struct WallpaperPackage {
    pub manifest: Manifest,
    /// 着色器源码（shader 类型）
    #[allow(dead_code)]
    pub shader_source: Option<String>,
    /// 图片数据（image 类型）
    pub image_data: Option<Vec<u8>>,
    /// 图片文件名（用于推断格式）
    pub image_name: Option<String>,
    /// 视频临时文件路径（video 类型，解压到临时目录；传给 VideoWallpaper）
    pub video_path: Option<std::path::PathBuf>,
    /// 视频临时文件路径的清理句柄（Drop 时删除，避免 .rwp 临时文件泄漏）
    pub temp_video_path: Option<std::path::PathBuf>,
    /// 音频数据
    pub audio_data: Option<Vec<u8>>,
    /// 音频文件名
    pub audio_name: Option<String>,
}

impl WallpaperPackage {
    /// 从 .rwp 文件加载
    pub fn load(path: &Path) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("zip: {e}")))?;

        let mut manifest: Option<Manifest> = None;
        let mut shader_source: Option<String> = None;
        let mut image_data: Option<Vec<u8>> = None;
        let mut image_name: Option<String> = None;
        let mut video_path: Option<std::path::PathBuf> = None;
        let mut temp_video_path: Option<std::path::PathBuf> = None;
        let mut audio_data: Option<Vec<u8>> = None;
        let mut audio_name: Option<String> = None;

        // 先读 manifest
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| std::io::Error::other(format!("zip entry: {e}")))?;
            let name = entry.name().to_string();

            if name == "manifest.json" || name.ends_with("/manifest.json") {
                let mut buf = String::new();
                entry.read_to_string(&mut buf)?;
                manifest = Some(serde_json::from_str(&buf)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("manifest: {e}")))?);
                break;
            }
        }

        let manifest = manifest.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "manifest.json not found"))?;

        // 根据类型读取资源
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| std::io::Error::other(format!("zip entry: {e}")))?;
            let name = entry.name().to_string();
            let lower = name.to_lowercase();

            // 着色器
            if lower == "shader.wgsl" || lower.ends_with("/shader.wgsl") {
                let mut buf = String::new();
                entry.read_to_string(&mut buf)?;
                shader_source = Some(buf);
                continue;
            }

            // 音频
            if let Some(audio_file) = &manifest.audio {
                if name == *audio_file || name.ends_with(audio_file) {
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf)?;
                    audio_data = Some(buf);
                    audio_name = Some(audio_file.clone());
                    continue;
                }
            }

            // 图片
            if manifest.wallpaper_type == "image"
                && (lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
                    || lower.ends_with(".bmp") || lower.ends_with(".webp") || lower.ends_with(".gif")) {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                image_data = Some(buf);
                image_name = Some(name);
                continue;
            }

            // 视频 — 解压到临时文件
            if manifest.wallpaper_type == "video"
                && (lower.ends_with(".mp4") || lower.ends_with(".mkv") || lower.ends_with(".avi")
                    || lower.ends_with(".webm") || lower.ends_with(".mov") || lower.ends_with(".flv")) {
                let ext = Path::new(&name).extension()
                    .and_then(|e| e.to_str()).unwrap_or("mp4");
                // 用纳秒戳作为随机后缀，避免多实例/重入时撞名
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let tmp = std::env::temp_dir().join(format!("rpaper_video_{}_{}.{}",
                    std::process::id(), nanos, ext));
                let mut file = std::fs::File::create(&tmp)?;
                io::copy(&mut entry, &mut file)?;
                video_path = Some(tmp.clone());
                temp_video_path = Some(tmp);
                continue;
            }
        }

        Ok(Self {
            manifest, shader_source, image_data, image_name,
            video_path, temp_video_path, audio_data, audio_name,
        })
    }

    /// 创建示例壁纸包（用于文档/测试）
    #[allow(dead_code)]
    pub fn create_example(output: &Path) -> io::Result<()> {
        let manifest = Manifest {
            name: "示例极光".into(),
            wallpaper_type: "shader".into(),
            author: "Rpaper".into(),
            description: "默认极光效果".into(),
            audio: None,
            params: None,
        };

        let file = std::fs::File::create(output)?;
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();

        zip.start_file("manifest.json", opts)?;
        let data = serde_json::to_vec_pretty(&manifest).unwrap();
        std::io::Write::write_all(&mut zip, &data)?;
        zip.finish()?;

        Ok(())
    }
}

/// 释放时清理解压出的视频临时文件，避免 .rwp 资源泄漏。
/// 注意：video_path 会被 VideoWallpaper::load 消费（移出），这里仅清理 temp_video_path。
impl Drop for WallpaperPackage {
    fn drop(&mut self) {
        if let Some(p) = &self.temp_video_path {
            // 文件可能已被删除或正被占用（Windows 下会失败），忽略错误
            let _ = std::fs::remove_file(p);
        }
    }
}
