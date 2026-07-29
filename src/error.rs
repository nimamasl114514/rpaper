//! 统一错误类型 — 分类友好提示
//!
//! 设计原则: 每个错误类别对应一个用户可理解的原因和建议，
//! 避免直接把 Rust 内部错误（如 "Os { code: 2, kind: NotFound ...}"）丢给用户。

use std::fmt;

#[derive(Debug)]
pub enum RpaperError {
    /// 文件不存在或无权限访问
    FileNotFound(String),
    /// 不支持的文件格式
    UnsupportedFormat(String),
    /// 解码失败（H.264 解码、MP4 demux、YUV 转换等）
    DecodeFailed(String),
    /// GPU/Surface 初始化失败
    GpuFailed(String),
    /// 壁纸包 (.rwp / .pkg) 结构无效
    InvalidPackage(String),
    /// 其他未分类错误
    Other(String),
}

impl RpaperError {
    /// 把任意字符串包装成最匹配的 RpaperError
    /// 通过关键词识别常见错误模式
    pub fn from_message(msg: impl Into<String>) -> Self {
        let s = msg.into();
        let lower = s.to_lowercase();
        // 文件不存在（仅匹配明确的文件系统错误关键词，避免误判"找不到 WorkerW"等）
        if lower.contains("no such file")
            || lower.contains("系统找不到指定的文件")
            || lower.contains("文件不存在")
            || lower.contains("the system cannot find")
        {
            return RpaperError::FileNotFound(s);
        }
        // 不支持的格式
        if lower.contains("不支持的文件格式")
            || lower.contains("不支持的格式")
            || lower.contains("unsupported")
            || lower.contains("unknown wallpaper")
            || lower.contains("未知壁纸类型")
            // PkgError::UnsupportedType 输出特征 — 区分 WE 类型不支持 vs 文件损坏
            || lower.contains("不支持 wallpaper engine")
        {
            return RpaperError::UnsupportedFormat(s);
        }
        // 解码失败
        if lower.contains("解码")
            || lower.contains("decode")
            || lower.contains("demux")
            || lower.contains("h264")
            || lower.contains("mp4")
            || lower.contains("avc")
            || lower.contains("sps")
            || lower.contains("pps")
            || lower.contains("nal")
            || lower.contains("yuv")
        {
            return RpaperError::DecodeFailed(s);
        }
        // GPU
        if lower.contains("gpu")
            || lower.contains("surface")
            || lower.contains("adapter")
            || lower.contains("device")
            || lower.contains("wgpu")
            || lower.contains("directx")
            || lower.contains("dx12")
        {
            return RpaperError::GpuFailed(s);
        }
        // 壁纸包
        if lower.contains("pkg")
            || lower.contains("rwp")
            || lower.contains("manifest")
            || lower.contains("壁纸包")
            || lower.contains("zip")
            || lower.contains("lz4")
        {
            return RpaperError::InvalidPackage(s);
        }
        RpaperError::Other(s)
    }
}

impl fmt::Display for RpaperError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RpaperError::FileNotFound(s) => write!(
                f,
                "找不到文件\n\n{s}\n\n请检查文件路径是否正确，或文件是否被移动/删除/重命名。"
            ),
            RpaperError::UnsupportedFormat(s) => write!(
                f,
                "不支持的格式\n\n{s}\n\nrpaper 支持:\n  图片: PNG/JPG/BMP/WEBP/GIF\n  视频: MP4/MKV/AVI/WebM/MOV\n  壁纸包: .rwp / Wallpaper Engine .pkg (仅 video 类型)"
            ),
            RpaperError::DecodeFailed(s) => write!(
                f,
                "解码失败\n\n{s}\n\n可能原因:\n  · 文件损坏\n  · 视频使用了 rpaper 不支持的 H.264 特性\n  · 容器格式不兼容\n\n建议尝试其他文件，或用 ffmpeg 重新转码为标准 MP4 (H.264 Baseline/Main)。"
            ),
            RpaperError::GpuFailed(s) => write!(
                f,
                "GPU 初始化失败\n\n{s}\n\n请确认:\n  · 显卡支持 DirectX 12\n  · 显卡驱动已更新到最新\n  · 没有其他程序独占 GPU"
            ),
            RpaperError::InvalidPackage(s) => write!(
                f,
                "壁纸包无效\n\n{s}\n\n请确认:\n  · .rwp 文件是 rpaper 标准壁纸包 (ZIP 含 manifest.json)\n  · .pkg 文件是 Wallpaper Engine 的 video 类型壁纸包\n  · 文件未损坏"
            ),
            RpaperError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl From<std::io::Error> for RpaperError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            RpaperError::FileNotFound(e.to_string())
        } else {
            RpaperError::from_message(e.to_string())
        }
    }
}

impl From<String> for RpaperError {
    fn from(s: String) -> Self {
        RpaperError::from_message(s)
    }
}

impl From<&str> for RpaperError {
    fn from(s: &str) -> Self {
        RpaperError::from_message(s)
    }
}

impl std::error::Error for RpaperError {}
