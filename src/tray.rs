//! 系统托盘图标管理

use std::io;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::core::w;
use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows::Win32::UI::WindowsAndMessaging::{
    LoadIconW, IDI_APPLICATION, CreatePopupMenu, TrackPopupMenu, AppendMenuW, DestroyMenu,
    GetCursorPos, SetForegroundWindow, PostMessageW, WM_COMMAND,
    TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD,
    MF_STRING, MF_SEPARATOR, MF_CHECKED, HMENU,
};

pub const WM_TRAYICON: u32 = 0x8000;
pub const IDM_AURORA: usize = 1001;
pub const IDM_PARTICLES: usize = 1002;
pub const IDM_IMAGE: usize = 1003;
pub const IDM_VIDEO: usize = 1005;
pub const IDM_PACKAGE: usize = 1006;
pub const IDM_EXIT: usize = 1004;

pub struct TrayIcon { hwnd: HWND }

impl TrayIcon {
    pub fn new(hwnd: HWND) -> io::Result<Self> {
        unsafe {
            let icon = LoadIconW(None, IDI_APPLICATION)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{e}")))?;

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
                return Err(io::Error::new(io::ErrorKind::Other, "Shell_NotifyIconW failed"));
            }
            Ok(Self { hwnd })
        }
    }

    pub fn show_menu(&self, current_wallpaper: u32, has_image: bool, has_video: bool) {
        unsafe {
            let hmenu = CreatePopupMenu().unwrap_or(HMENU(0));
            if hmenu.0 == 0 { return; }

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
            let _ = AppendMenuW(hmenu, MF_STRING, IDM_EXIT, w!("退出"));

            let mut pt = windows::Win32::Foundation::POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.hwnd);

            let cmd = TrackPopupMenu(
                hmenu, TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RETURNCMD,
                pt.x, pt.y, 0, self.hwnd, None,
            );
            let cmd_id = cmd.0 as i32;
            if cmd_id != 0 {
                let _ = PostMessageW(self.hwnd, WM_COMMAND, WPARAM(cmd_id as usize), LPARAM(0));
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
