//! 系统壁纸接管 — 启动时保存原壁纸并设为纯色（避免透出原壁纸造成色差），
//! 退出时恢复原壁纸。这是 Wallpaper Engine 等动态壁纸软件的通用做法。
//!
//! 用 IDesktopWallpaper COM 接口（Windows 8+，windows crate 已含）。

use windows::core::PCWSTR;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper, DWPOS_CENTER};

/// 系统壁纸守卫 — RAII 模式，Drop 时自动恢复原壁纸
pub struct SysWallpaperGuard {
    /// 启动时保存的原壁纸路径（空字符串表示原本就是纯色）
    original: Option<String>,
    /// COM 是否由本实例初始化（用于决定退出时是否 CoUninitialize）
    com_inited: bool,
}

impl SysWallpaperGuard {
    /// 保存当前系统壁纸并设为纯色黑色
    pub fn take() -> Self {
        // 初始化 COM（已初始化时返回 RPC_E_CHANGED_MODE，忽略即可）
        let com_inited = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();

        let dw: IDesktopWallpaper = match unsafe {
            CoCreateInstance::<_, IDesktopWallpaper>(&DesktopWallpaper, None, CLSCTX_ALL)
        } {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[rpaper] 创建 IDesktopWallpaper 失败: {e}");
                return Self { original: None, com_inited };
            }
        };

        // 保存原壁纸路径
        let original = unsafe { dw.GetWallpaper(PCWSTR::null()) }
            .ok()
            .and_then(|p| unsafe { p.to_string() }.ok())
            .filter(|s| !s.is_empty());

        // 设为空字符串 → 系统显示纯色背景
        let _ = unsafe { dw.SetWallpaper(PCWSTR::null(), PCWSTR::null()) };

        // 设为纯黑背景色（避免原壁纸残留色）
        let _ = unsafe { dw.SetBackgroundColor(windows::Win32::Foundation::COLORREF(0)) };

        // 居中显示（避免被系统拉伸）
        let _ = unsafe { dw.SetPosition(DWPOS_CENTER) };

        Self { original, com_inited }
    }
}

impl Drop for SysWallpaperGuard {
    fn drop(&mut self) {
        if let Some(path) = &self.original {
            if let Ok(dw) = unsafe {
                CoCreateInstance::<_, IDesktopWallpaper>(&DesktopWallpaper, None, CLSCTX_ALL)
            } {
                let path_w: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = unsafe { dw.SetWallpaper(PCWSTR::null(), PCWSTR(path_w.as_ptr())) };
            }
        }
        // 释放 COM（仅当本实例初始化了 COM）
        if self.com_inited {
            unsafe { CoUninitialize() };
        }
    }
}
