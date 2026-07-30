//! 系统托盘图标管理

use std::io;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::core::w;
use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, TrackPopupMenu, AppendMenuW, DestroyMenu,
    GetCursorPos, SetForegroundWindow, PostMessageW, WM_COMMAND,
    TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD,
    MF_STRING, MF_SEPARATOR, MF_CHECKED, HMENU,
};
use crate::settings::load_app_icon;

pub const WM_TRAYICON: u32 = 0x8000;
pub const IDM_AURORA: usize = 1001;
pub const IDM_PARTICLES: usize = 1002;
pub const IDM_IMAGE: usize = 1003;
pub const IDM_VIDEO: usize = 1005;
pub const IDM_PACKAGE: usize = 1006;
pub const IDM_EXIT: usize = 1004;
pub const IDM_SETTINGS: usize = 1007;
pub const IDM_LIBRARY: usize = 1008;

pub struct TrayIcon { hwnd: HWND }

impl TrayIcon {
    pub fn new(hwnd: HWND) -> io::Result<Self> {
        unsafe {
            // 使用自定义蓝紫 R 图标（16px 小图标，适合托盘显示）
            // 托盘图标推荐 16px，系统会自动缩放到 DPI 适配尺寸
            let icon = load_app_icon();
            if icon.0.is_null() {
                return Err(std::io::Error::other("加载托盘图标失败"));
            }

            let tip: Vec<u16> = "Rpaper\0".encode_utf16().collect();
            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd, uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_TRAYICON, hIcon: icon,
                ..Default::default()
            };
            let tip_len = tip.len().min(128);
            nid.szTip[..tip_len].copy_from_slice(&tip[..tip_len]);

            if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                return Err(std::io::Error::other("Shell_NotifyIconW failed"));
            }
            Ok(Self { hwnd })
        }
    }

    pub fn show_menu(&self, current_wallpaper: u32, has_image: bool, has_video: bool) {
        unsafe {
            let hmenu = CreatePopupMenu().unwrap_or(HMENU(std::ptr::null_mut()));
            if hmenu.0.is_null() { return; }

            let mk_flags = |checked: bool| {
                if checked { MF_STRING | MF_CHECKED } else { MF_STRING }
            };

            let _ = AppendMenuW(hmenu, mk_flags(current_wallpaper == 0), IDM_AURORA, w!("极光效果"));
            let _ = AppendMenuW(hmenu, mk_flags(current_wallpaper == 1), IDM_PARTICLES, w!("粒子效果"));
            let img_label = if has_image { w!("自定义图片 (已加载)") } else { w!("选择图片...") };
            let _ = AppendMenuW(hmenu, mk_flags(current_wallpaper == 2), IDM_IMAGE, img_label);
            let vid_label = if has_video { w!("视频壁纸 (已加载)") } else { w!("选择视频...") };
            let _ = AppendMenuW(hmenu, mk_flags(current_wallpaper == 3), IDM_VIDEO, vid_label);
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, w!(""));
            let _ = AppendMenuW(hmenu, MF_STRING, IDM_PACKAGE, w!("加载壁纸包 (.rwp)..."));
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, w!(""));
            let _ = AppendMenuW(hmenu, MF_STRING, IDM_LIBRARY, w!("壁纸库..."));
            let _ = AppendMenuW(hmenu, MF_STRING, IDM_SETTINGS, w!("快速设置..."));
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, w!(""));
            let _ = AppendMenuW(hmenu, MF_STRING, IDM_EXIT, w!("退出"));

            let mut pt = windows::Win32::Foundation::POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.hwnd);

            let cmd = TrackPopupMenu(
                hmenu, TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RETURNCMD,
                pt.x, pt.y, Some(0), self.hwnd, None,
            );
            let cmd_id = cmd.0 as i32;
            if cmd_id != 0 {
                let _ = PostMessageW(Some(self.hwnd), WM_COMMAND, WPARAM(cmd_id as usize), LPARAM(0));
            }
            let _ = DestroyMenu(hmenu);
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            let nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd, uID: 1,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }
}
