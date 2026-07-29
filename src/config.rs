//! 应用配置持久化 — JSON 文件存储到 %APPDATA%\rpaper\config.json

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 当前壁纸类型: "aurora" / "particles" / "image" / "video"
    #[serde(default = "default_wallpaper")]
    pub wallpaper_type: String,
    /// 音量 0.0..=1.0
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// 开机自启
    #[serde(default)]
    pub autostart: bool,
    /// 上次加载的图片路径
    #[serde(default)]
    pub last_image_path: Option<String>,
    /// 上次加载的视频路径
    #[serde(default)]
    pub last_video_path: Option<String>,
    /// 上次加载的音频路径
    #[serde(default)]
    pub last_audio_path: Option<String>,
    /// 上次加载的壁纸包路径
    #[serde(default)]
    pub last_package_path: Option<String>,
}

fn default_wallpaper() -> String { "aurora".into() }
fn default_volume() -> f32 { 0.5 }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            wallpaper_type: default_wallpaper(),
            volume: default_volume(),
            autostart: false,
            last_image_path: None,
            last_video_path: None,
            last_audio_path: None,
            last_package_path: None,
        }
    }
}

impl AppConfig {
    /// 配置文件路径: %APPDATA%\rpaper\config.json
    pub fn config_path() -> PathBuf {
        let appdata = std::env::var("APPDATA")
            .unwrap_or_else(|_| ".".into());
        PathBuf::from(appdata).join("rpaper").join("config.json")
    }

    /// 加载配置，失败时返回默认值（不报错）
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str::<AppConfig>(&s) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[rpaper] 配置文件解析失败，使用默认配置: {e}");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    /// 保存配置（原子写入：先写临时文件再重命名）
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建配置目录失败: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("序列化配置失败: {e}"))?;

        // 原子写入：临时文件 + rename
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)
            .map_err(|e| format!("写入配置文件失败: {e}"))?;
        std::fs::rename(&tmp, &path)
            .map_err(|e| format!("重命名配置文件失败: {e}"))?;
        Ok(())
    }
}
