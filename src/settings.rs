//! 设置界面 — Win32 原生窗口

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM, LRESULT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassExW, ShowWindow, DestroyWindow,
    SendMessageW, PostMessageW, GetDlgItem, GetParent,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW, HMENU,
    WS_OVERLAPPED, WS_CAPTION, WS_SYSMENU, WS_CHILD, WS_VISIBLE,
    WM_CREATE, WM_COMMAND, WM_DESTROY, WM_HSCROLL, WM_SETTEXT,
    WM_GETMINMAXINFO, MINMAXINFO, SW_SHOW,
    BS_PUSHBUTTON, BS_AUTORADIOBUTTON, BS_GROUPBOX,
};
use windows::Win32::UI::Controls::{
    INITCOMMONCONTROLSEX, ICC_BAR_CLASSES, InitCommonControlsEx,
    TBS_HORZ, TBS_AUTOTICKS,
};
use windows::Win32::Graphics::Gdi::{GetStockObject, WHITE_BRUSH, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use std::cell::RefCell;

thread_local! {
    static HIDDEN_HWND: RefCell<Option<HWND>> = RefCell::new(None);
}

/// 设置隐藏窗口 HWND，供设置窗口转发消息
pub fn set_hidden_hwnd(hwnd: HWND) {
    HIDDEN_HWND.with(|h| *h.borrow_mut() = Some(hwnd));
}

const TBM_SETRANGE: u32 = 0x0406;
const TBM_SETPOS: u32 = 0x0405;
const TBM_GETPOS: u32 = 0x0400;
const SS_LEFT: u32 = 0x00000000;

pub const IDC_VOLUME_SLIDER: u16 = 1001;
pub const IDC_VOLUME_LABEL: u16 = 1002;
pub const IDC_RADIO_AURORA: u16 = 1003;
pub const IDC_RADIO_PARTICLES: u16 = 1004;
pub const IDC_RADIO_IMAGE: u16 = 1005;
pub const IDC_RADIO_VIDEO: u16 = 1006;
pub const IDC_BTN_PAUSE: u16 = 1007;
pub const IDC_BTN_SELECT_IMAGE: u16 = 1008;
pub const IDC_BTN_SELECT_VIDEO: u16 = 1009;
pub const IDC_BTN_SELECT_PACKAGE: u16 = 1010;
pub const IDC_BTN_CLOSE: u16 = 1011;
pub const IDC_GROUP_WALLPAPER: u16 = 1012;
pub const IDC_GROUP_VOLUME: u16 = 1013;
pub const IDC_BTN_OPEN_AUDIO: u16 = 1014;
pub const IDC_AUDIO_LABEL: u16 = 1015;
pub const IDC_CHECK_AUTOSTART: u16 = 1016;

pub const CMD_OPEN_SETTINGS: usize = 2001;
pub const CMD_VOLUME_CHANGED: usize = 2002;
pub const CMD_WALLPAPER_CHANGED: usize = 2003;
pub const CMD_PAUSE_TOGGLE: usize = 2004;
pub const CMD_SELECT_IMAGE: usize = 2005;
pub const CMD_SELECT_VIDEO: usize = 2006;
pub const CMD_SELECT_PACKAGE: usize = 2007;
pub const CMD_OPEN_AUDIO: usize = 2008;
pub const CMD_AUTOSTART_TOGGLE: usize = 2009;

pub struct SettingsWindow {
    pub hwnd: HWND,
}

impl SettingsWindow {
    pub fn create(parent: HWND) -> Result<Self, String> {
        unsafe {
            let icc = INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_BAR_CLASSES,
            };
            let _ = InitCommonControlsEx(&icc);

            let hinst = GetModuleHandleW(None).map_err(|e| format!("{e}"))?;
            let class_name = w!("RpaperSettings");

            let wcex = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(settings_wnd_proc),
                hInstance: hinst.into(),
                hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
                lpszClassName: class_name,
                ..Default::default()
            };
            let _ = RegisterClassExW(&wcex);

            let title: Vec<u16> = "Rpaper 设置\0".encode_utf16().collect();
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
                CW_USEDEFAULT, CW_USEDEFAULT,
                420, 520,
                parent, HMENU(0), hinst, None,
            );

            if hwnd.0 == 0 {
                return Err("创建设置窗口失败".into());
            }

            Ok(Self { hwnd })
        }
    }

    pub fn show(&self) {
        unsafe { let _ = ShowWindow(self.hwnd, SW_SHOW); }
    }
}

unsafe fn create_ctrl(
    class: PCWSTR, text: &str, style: u32,
    x: i32, y: i32, w: i32, h: i32, id: u16, parent: HWND,
) {
    let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let hinst = GetModuleHandleW(None).unwrap();
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE(0), class, PCWSTR(text_wide.as_ptr()),
        WINDOW_STYLE(style),
        x, y, w, h,
        parent, HMENU(id as isize), hinst, None,
    );
}

extern "system" fn settings_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                let h = hwnd;
                let cs: u32 = WS_CHILD.0 | WS_VISIBLE.0;

                create_ctrl(w!("BUTTON"), "壁纸效果", BS_GROUPBOX as u32 | cs,
                    10, 10, 380, 130, IDC_GROUP_WALLPAPER, h);
                create_ctrl(w!("BUTTON"), "极光效果", BS_AUTORADIOBUTTON as u32 | cs,
                    25, 35, 100, 25, IDC_RADIO_AURORA, h);
                create_ctrl(w!("BUTTON"), "粒子效果", BS_AUTORADIOBUTTON as u32 | cs,
                    135, 35, 100, 25, IDC_RADIO_PARTICLES, h);
                create_ctrl(w!("BUTTON"), "图片壁纸", BS_AUTORADIOBUTTON as u32 | cs,
                    25, 65, 100, 25, IDC_RADIO_IMAGE, h);
                create_ctrl(w!("BUTTON"), "视频壁纸", BS_AUTORADIOBUTTON as u32 | cs,
                    135, 65, 100, 25, IDC_RADIO_VIDEO, h);
                create_ctrl(w!("BUTTON"), "选择图片...", BS_PUSHBUTTON as u32 | cs,
                    25, 100, 90, 28, IDC_BTN_SELECT_IMAGE, h);
                create_ctrl(w!("BUTTON"), "选择视频...", BS_PUSHBUTTON as u32 | cs,
                    125, 100, 90, 28, IDC_BTN_SELECT_VIDEO, h);
                create_ctrl(w!("BUTTON"), "加载壁纸包...", BS_PUSHBUTTON as u32 | cs,
                    225, 100, 120, 28, IDC_BTN_SELECT_PACKAGE, h);

                create_ctrl(w!("BUTTON"), "音频", BS_GROUPBOX as u32 | cs,
                    10, 150, 380, 100, IDC_GROUP_VOLUME, h);
                create_ctrl(w!("STATIC"), "音量: 50%", SS_LEFT | cs,
                    25, 175, 100, 20, IDC_VOLUME_LABEL, h);
                create_ctrl(w!("msctls_trackbar32"), "",
                    TBS_HORZ | TBS_AUTOTICKS | cs,
                    130, 170, 240, 30, IDC_VOLUME_SLIDER, h);

                let slider = GetDlgItem(h, IDC_VOLUME_SLIDER as i32);
                if slider.0 != 0 {
                    // TBM_SETRANGE: lParam = MAKELONG(min, max) = min | (max << 16)
                    let _ = SendMessageW(slider, TBM_SETRANGE, WPARAM(1), LPARAM(0 | (100 << 16)));
                    let _ = SendMessageW(slider, TBM_SETPOS, WPARAM(1), LPARAM(50));
                }

                create_ctrl(w!("BUTTON"), "选择背景音乐...", BS_PUSHBUTTON as u32 | cs,
                    25, 210, 150, 28, IDC_BTN_OPEN_AUDIO, h);
                create_ctrl(w!("STATIC"), "未加载", SS_LEFT | cs,
                    185, 215, 180, 20, IDC_AUDIO_LABEL, h);

                create_ctrl(w!("BUTTON"), "其他设置", BS_GROUPBOX as u32 | cs,
                    10, 260, 380, 80, 1017, h);
                create_ctrl(w!("BUTTON"), "开机自动启动", BS_AUTORADIOBUTTON as u32 | cs,
                    25, 285, 150, 25, IDC_CHECK_AUTOSTART, h);
                create_ctrl(w!("BUTTON"), "暂停壁纸", BS_PUSHBUTTON as u32 | cs,
                    25, 310, 130, 28, IDC_BTN_PAUSE, h);

                create_ctrl(w!("BUTTON"), "关闭设置", BS_PUSHBUTTON as u32 | cs,
                    290, 460, 100, 32, IDC_BTN_CLOSE, h);

                LRESULT(0)
            }
            WM_COMMAND => {
                let cmd = (wparam.0 as u32) & 0xFFFF;
                // 无父窗口时，转发到隐藏窗口（消息处理中心）
                let target = {
                    let parent = GetParent(hwnd);
                    if parent.0 != 0 { parent } else { HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(0))) }
                };

                match cmd as u16 {
                    IDC_BTN_CLOSE => { let _ = DestroyWindow(hwnd); }
                    IDC_RADIO_AURORA | IDC_RADIO_PARTICLES | IDC_RADIO_IMAGE | IDC_RADIO_VIDEO => {
                        let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_WALLPAPER_CHANGED), LPARAM(cmd as isize));
                    }
                    IDC_BTN_SELECT_IMAGE => {
                        let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_SELECT_IMAGE), LPARAM(0));
                    }
                    IDC_BTN_SELECT_VIDEO => {
                        let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_SELECT_VIDEO), LPARAM(0));
                    }
                    IDC_BTN_SELECT_PACKAGE => {
                        let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_SELECT_PACKAGE), LPARAM(0));
                    }
                    IDC_BTN_OPEN_AUDIO => {
                        let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_OPEN_AUDIO), LPARAM(0));
                    }
                    IDC_BTN_PAUSE => {
                        let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_PAUSE_TOGGLE), LPARAM(0));
                    }
                    IDC_CHECK_AUTOSTART => {
                        let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_AUTOSTART_TOGGLE), LPARAM(0));
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_HSCROLL => {
                let slider = GetDlgItem(hwnd, IDC_VOLUME_SLIDER as i32);
                if slider.0 != 0 {
                    let pos = SendMessageW(slider, TBM_GETPOS, WPARAM(0), LPARAM(0));
                    let vol = pos.0 as u32;

                    let label_text = format!("音量: {}%\0", vol);
                    let label_wide: Vec<u16> = label_text.encode_utf16().collect();
                    let label = GetDlgItem(hwnd, IDC_VOLUME_LABEL as i32);
                    if label.0 != 0 {
                        let _ = SendMessageW(label, WM_SETTEXT, WPARAM(0), LPARAM(label_wide.as_ptr() as isize));
                    }

                    let target = {
                        let parent = GetParent(hwnd);
                        if parent.0 != 0 { parent } else { HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(0))) }
                    };
                    let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_VOLUME_CHANGED), LPARAM(vol as isize));
                }
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let mmi = lparam.0 as *mut MINMAXINFO;
                if !mmi.is_null() {
                    (*mmi).ptMinTrackSize.x = 420;
                    (*mmi).ptMinTrackSize.y = 520;
                    (*mmi).ptMaxTrackSize.x = 420;
                    (*mmi).ptMaxTrackSize.y = 520;
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                // 通知主窗口设置窗口已关闭
                HIDDEN_HWND.with(|h| {
                    let hidden = h.borrow().unwrap_or(HWND(0));
                    if hidden.0 != 0 {
                        let _ = PostMessageW(hidden, WM_COMMAND, WPARAM(0xDEAD), LPARAM(0));
                    }
                });
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub fn update_wallpaper_selection(hwnd: HWND, wallpaper_id: u32) {
    unsafe {
        let radio = match wallpaper_id {
            0 => IDC_RADIO_AURORA,
            1 => IDC_RADIO_PARTICLES,
            2 => IDC_RADIO_IMAGE,
            3 => IDC_RADIO_VIDEO,
            _ => return,
        };
        for &id in &[IDC_RADIO_AURORA, IDC_RADIO_PARTICLES, IDC_RADIO_IMAGE, IDC_RADIO_VIDEO] {
            let h = GetDlgItem(hwnd, id as i32);
            if h.0 != 0 {
                let _ = SendMessageW(h,
                    windows::Win32::UI::WindowsAndMessaging::BM_SETCHECK,
                    WPARAM(if id == radio { 1 } else { 0 }), LPARAM(0));
            }
        }
    }
}

pub fn update_audio_label(hwnd: HWND, text: &str) {
    unsafe {
        let full = format!("{}\0", text);
        let wide: Vec<u16> = full.encode_utf16().collect();
        let h = GetDlgItem(hwnd, IDC_AUDIO_LABEL as i32);
        if h.0 != 0 {
            let _ = SendMessageW(h, WM_SETTEXT, WPARAM(0), LPARAM(wide.as_ptr() as isize));
        }
    }
}

pub fn update_pause_button(hwnd: HWND, paused: bool) {
    unsafe {
        let text = if paused { "恢复壁纸\0" } else { "暂停壁纸\0" };
        let wide: Vec<u16> = text.encode_utf16().collect();
        let h = GetDlgItem(hwnd, IDC_BTN_PAUSE as i32);
        if h.0 != 0 {
            let _ = SendMessageW(h, WM_SETTEXT, WPARAM(0), LPARAM(wide.as_ptr() as isize));
        }
    }
}
