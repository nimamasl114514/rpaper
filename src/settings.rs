//! 设置界面 — 简化版（保留核心功能，兼容 windows 0.62）
//! 后续将用 Slint 重写为完整的 Fluent Design 壁纸库窗口

use std::ptr;
use windows::core::PCWSTR;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, HINSTANCE, LPARAM, WPARAM, LRESULT, HMODULE, COLORREF, NO_ERROR};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, HFONT,
    FW_NORMAL, FW_SEMIBOLD, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS,
    CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, DEFAULT_PITCH,
    TRANSPARENT, SetBkMode, SetTextColor, SetBkColor,
};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassExW, ShowWindow, DestroyWindow,
    SendMessageW, PostMessageW, GetDlgItem, GetParent, LoadImageW,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW, HMENU,
    WS_OVERLAPPEDWINDOW, WS_CHILD, WS_VISIBLE,
    WM_CREATE, WM_COMMAND, WM_DESTROY, WM_HSCROLL, WM_SETTEXT,
    WM_CTLCOLORDLG, WM_CLOSE, WM_CTLCOLORSTATIC,
    WM_KEYDOWN, WM_SETFONT,
    BS_PUSHBUTTON, BS_AUTOCHECKBOX,
    BM_SETCHECK, HICON,
    IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
    IDC_ARROW, LoadCursorW,
    SW_SHOW,
};
// SS_LEFT 原始值 — windows 0.62 中不在 WindowsAndMessaging 模块
const SS_LEFT: u32 = 0x00000000;
use windows::Win32::UI::Controls::{
    INITCOMMONCONTROLSEX, ICC_BAR_CLASSES, ICC_PROGRESS_CLASS, InitCommonControlsEx,
    TBS_HORZ, PROGRESS_CLASSW, TRACKBAR_CLASSW,
    PBM_SETRANGE32, PBM_SETPOS, SetWindowTheme, TBS_AUTOTICKS, TBS_TOOLTIPS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, RegCloseKey,
    HKEY,
};
use windows::core::w;
use std::cell::RefCell;

// === 常量 ===
const VK_ESCAPE: u16 = 0x1B;
const VK_SPACE: u16 = 0x20;
const TBM_GETPOS: u32 = 0x0400;
const TBM_SETPOS: u32 = 0x0405;
const TBM_SETRANGE: u32 = 0x0406;
const PBS_SMOOTH: u32 = 0x01;
// BST_CHECKED 原始值
const BST_CHECKED: usize = 1;
// 窗口样式 — 不使用缺失的常量，直接用 WS_OVERLAPPEDWINDOW 简化
const WIN_STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0 & !(0x00040000 | 0x00020000)); // 去掉 WS_THICKFRAME | WS_MAXIMIZEBOX
const WIN_W: i32 = 480;
const WIN_H: i32 = 520;

// === 控件 ID ===
pub const IDC_VOLUME_SLIDER: u16 = 1001;
pub const IDC_VOLUME_LABEL: u16 = 1002;
pub const IDC_CARD_AURORA: u16 = 1003;
pub const IDC_CARD_PARTICLES: u16 = 1004;
pub const IDC_CARD_IMAGE: u16 = 1005;
pub const IDC_CARD_VIDEO: u16 = 1006;
pub const IDC_BTN_PAUSE: u16 = 1007;
pub const IDC_BTN_SELECT_IMAGE: u16 = 1008;
pub const IDC_BTN_SELECT_VIDEO: u16 = 1009;
pub const IDC_BTN_SELECT_PACKAGE: u16 = 1010;
pub const IDC_BTN_CLOSE: u16 = 1011;
pub const IDC_BTN_OPEN_AUDIO: u16 = 1014;
pub const IDC_AUDIO_LABEL: u16 = 1015;
pub const IDC_CHECK_AUTOSTART: u16 = 1016;
pub const IDC_VIDEO_STATUS: u16 = 1019;
pub const IDC_VIDEO_PROGRESS: u16 = 1020;
pub const IDC_CURRENT_FILE: u16 = 1021;
pub const IDC_TITLE1: u16 = 1111;
pub const IDC_TITLE2: u16 = 1112;
pub const IDC_TITLE3: u16 = 1113;

// === 命令码 ===
pub const CMD_OPEN_SETTINGS: usize = 2001;
pub const CMD_VOLUME_CHANGED: usize = 2002;
pub const CMD_WALLPAPER_CHANGED: usize = 2003;
pub const CMD_PAUSE_TOGGLE: usize = 2004;
pub const CMD_SELECT_IMAGE: usize = 2005;
pub const CMD_SELECT_VIDEO: usize = 2006;
pub const CMD_SELECT_PACKAGE: usize = 2007;
pub const CMD_OPEN_AUDIO: usize = 2008;
pub const CMD_AUTOSTART_TOGGLE: usize = 2009;
pub const CMD_SETTINGS_CLOSED: usize = 0xDEAD;

thread_local! {
    static HIDDEN_HWND: RefCell<Option<HWND>> = RefCell::new(None);
    static SETTINGS_HWND: RefCell<Option<HWND>> = RefCell::new(None);
    static DARK_MODE: RefCell<bool> = RefCell::new(true);
    static HFONT_TITLE: RefCell<Option<HFONT>> = RefCell::new(None);
    static HFONT_BODY: RefCell<Option<HFONT>> = RefCell::new(None);
    static CURRENT_VOLUME: RefCell<u32> = RefCell::new(50);
}

// HFONT 直接用 windows::Win32::Graphics::Gdi::HFONT（*mut c_void 句柄）

pub fn set_hidden_hwnd(hwnd: HWND) {
    HIDDEN_HWND.with(|h| *h.borrow_mut() = Some(hwnd));
}

pub fn load_app_icon() -> HICON {
    unsafe {
        // windows 0.62: LoadImageW 第一参数是 Option<HINSTANCE>，从 HMODULE 转换
        let hinstance = GetModuleHandleW(None).ok().map(|h| HINSTANCE(h.0));
        let icon = LoadImageW(
            hinstance,
            PCWSTR(1 as *const u16), // IDI_APP
            IMAGE_ICON,
            0, 0,
            LR_DEFAULTSIZE | LR_SHARED,
        );
        match icon {
            Ok(h) => HICON(h.0),
            Err(_) => HICON(ptr::null_mut()),
        }
    }
}

fn detect_dark_mode() -> bool {
    unsafe {
        let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
            .encode_utf16().collect();
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), None, KEY_READ, &mut hkey) == NO_ERROR {
            let mut val_type = 0u32;
            let mut data = 1u32;
            let mut size = std::mem::size_of::<u32>() as u32;
            let val_name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
            // windows 0.62: RegQueryValueExW 返回 WIN32_ERROR；lpdata 期望 *mut u8
            let _ = RegQueryValueExW(hkey, PCWSTR(val_name.as_ptr()), None,
                Some(&mut val_type as *mut u32 as *mut _), Some(&mut data as *mut u32 as *mut u8), Some(&mut size));
            let _ = RegCloseKey(hkey);
            data == 0 // 0 = dark theme
        } else {
            true // default dark
        }
    }
}

fn create_font(size: i32, bold: bool) -> HFONT {
    unsafe {
        // windows 0.62: cweight 是 i32，FW_* 是 FONT_WEIGHT newtype，需取 .0
        // bitalic/bunderline/bstrikeout 是 u32，false 不能隐式转，用 0
        // ipitchandfamily 是 u32，DEFAULT_PITCH 是 FONT_PITCH newtype，需取 .0
        CreateFontW(
            size, 0, 0, 0,
            if bold { FW_SEMIBOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
            0, 0, 0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Microsoft YaHei UI"),
        )
    }
}

unsafe fn create_label(
    parent: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, id: u16, bold: bool
) -> HWND {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        PCWSTR(wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_LEFT as u32),
        x, y, w, h,
        Some(parent),
        Some(HMENU(id as usize as *mut std::ffi::c_void)),
        GetModuleHandleW(None).ok().map(|h| HINSTANCE(h.0)),
        None,
    ).unwrap_or(HWND(ptr::null_mut()));
    let font = if bold { HFONT_TITLE.with(|f| *f.borrow()) } else { HFONT_BODY.with(|f| *f.borrow()) };
    if let Some(f) = font {
        let _ = SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(f.0 as usize)), Some(LPARAM(1)));
    }
    let _ = SetWindowTheme(hwnd, w!(""), w!(""));
    hwnd
}

unsafe fn create_button(
    parent: HWND, text: &str, x: i32, y: i32, w: i32, h: i32, id: u16, style: u32
) -> HWND {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("BUTTON"),
        PCWSTR(wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
        x, y, w, h,
        Some(parent),
        Some(HMENU(id as usize as *mut std::ffi::c_void)),
        GetModuleHandleW(None).ok().map(|h| HINSTANCE(h.0)),
        None,
    ).unwrap_or(HWND(ptr::null_mut()));
    if let Some(f) = HFONT_BODY.with(|f| *f.borrow()) {
        let _ = SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(f.0 as usize)), Some(LPARAM(1)));
    }
    hwnd
}

unsafe fn create_slider(parent: HWND, x: i32, y: i32, w: i32, h: i32, id: u16) -> HWND {
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        TRACKBAR_CLASSW,
        None,
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(TBS_HORZ as u32 | TBS_AUTOTICKS as u32 | TBS_TOOLTIPS as u32),
        x, y, w, h,
        Some(parent),
        Some(HMENU(id as usize as *mut std::ffi::c_void)),
        GetModuleHandleW(None).ok().map(|h| HINSTANCE(h.0)),
        None,
    ).unwrap_or(HWND(ptr::null_mut()));
    hwnd
}

unsafe fn create_progress(parent: HWND, x: i32, y: i32, w: i32, h: i32, id: u16) -> HWND {
    CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PROGRESS_CLASSW,
        None,
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(PBS_SMOOTH),
        x, y, w, h,
        Some(parent),
        Some(HMENU(id as usize as *mut std::ffi::c_void)),
        GetModuleHandleW(None).ok().map(|h| HINSTANCE(h.0)),
        None,
    ).unwrap_or(HWND(ptr::null_mut()))
}

pub struct SettingsWindow {
    pub hwnd: HWND,
}

impl SettingsWindow {
    pub fn create(parent: HWND) -> Result<Self, String> {
        unsafe {
            // 初始化通用控件
            let icc = INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_BAR_CLASSES | ICC_PROGRESS_CLASS,
            };
            let _ = InitCommonControlsEx(&icc);

            let dark = detect_dark_mode();
            DARK_MODE.with(|d| *d.borrow_mut() = dark);

            let hfont_title = create_font(18, true);
            let hfont_body = create_font(14, false);
            HFONT_TITLE.with(|f| *f.borrow_mut() = Some(hfont_title));
            HFONT_BODY.with(|f| *f.borrow_mut() = Some(hfont_body));

            let class_name = w!("RpaperSettingsWnd");

            let bg_color = if dark { 0x00202020u32 } else { 0x00F3F3F3u32 };
            let bg_brush = CreateSolidBrush(COLORREF(bg_color));

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(settings_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: HINSTANCE(GetModuleHandleW(None).unwrap_or(HMODULE(ptr::null_mut())).0),
                hIcon: load_app_icon(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: bg_brush,
                lpszMenuName: PCWSTR::null(),
                lpszClassName: class_name,
                hIconSm: load_app_icon(),
            };
            let _ = RegisterClassExW(&wc);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                w!("Rpaper 设置"),
                WIN_STYLE,
                CW_USEDEFAULT, CW_USEDEFAULT, WIN_W, WIN_H,
                if !parent.0.is_null() { Some(parent) } else { None },
                None,
                GetModuleHandleW(None).ok().map(|h| HINSTANCE(h.0)),
                None,
            ).map_err(|_| "创建设置窗口失败".to_string())?;

            if hwnd.0.is_null() {
                return Err("创建设置窗口失败".to_string());
            }

            // DWM 圆角 + 深色标题栏
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &DWMWCP_ROUND as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            );
            let dark_val: BOOL = if dark { BOOL(1) } else { BOOL(0) };
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_val as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<BOOL>() as u32,
            );

            create_controls(hwnd, dark);

            Ok(Self { hwnd })
        }
    }

    pub fn show(&self) {
        unsafe { let _ = ShowWindow(self.hwnd, SW_SHOW); }
    }
}

unsafe fn create_controls(hwnd: HWND, _dark: bool) {
    // 标题
    create_label(hwnd, "Rpaper 设置", 20, 16, 200, 30, IDC_TITLE1, true);
    create_label(hwnd, "选择壁纸", 20, 60, 200, 24, IDC_TITLE2, true);

    // 壁纸选择按钮
    create_button(hwnd, "极光", 20, 90, 100, 36, IDC_CARD_AURORA, BS_PUSHBUTTON as u32);
    create_button(hwnd, "粒子", 130, 90, 100, 36, IDC_CARD_PARTICLES, BS_PUSHBUTTON as u32);
    create_button(hwnd, "图片", 240, 90, 100, 36, IDC_CARD_IMAGE, BS_PUSHBUTTON as u32);
    create_button(hwnd, "视频", 350, 90, 100, 36, IDC_CARD_VIDEO, BS_PUSHBUTTON as u32);

    // 分隔线标签
    create_label(hwnd, "音量控制", 20, 150, 200, 24, IDC_TITLE3, true);
    create_label(hwnd, "50%", 400, 182, 50, 20, IDC_VOLUME_LABEL, false);

    // 音量滑块
    let slider = create_slider(hwnd, 20, 180, 370, 30, IDC_VOLUME_SLIDER);
    let _ = SendMessageW(slider, TBM_SETRANGE, Some(WPARAM(1)), Some(LPARAM((0i32 as isize) | ((100i32 as isize) << 16))));
    let _ = SendMessageW(slider, TBM_SETPOS, Some(WPARAM(1)), Some(LPARAM(50)));

    // 选项
    create_button(hwnd, "开机自动启动", 20, 230, 200, 28, IDC_CHECK_AUTOSTART, BS_AUTOCHECKBOX as u32);
    create_button(hwnd, "暂停/播放", 20, 270, 130, 32, IDC_BTN_PAUSE, BS_PUSHBUTTON as u32);
    create_button(hwnd, "选择图片", 160, 270, 130, 32, IDC_BTN_SELECT_IMAGE, BS_PUSHBUTTON as u32);
    create_button(hwnd, "选择视频", 300, 270, 130, 32, IDC_BTN_SELECT_VIDEO, BS_PUSHBUTTON as u32);
    create_button(hwnd, "导入 .pkg", 20, 310, 130, 32, IDC_BTN_SELECT_PACKAGE, BS_PUSHBUTTON as u32);
    create_button(hwnd, "音频设置", 160, 310, 130, 32, IDC_BTN_OPEN_AUDIO, BS_PUSHBUTTON as u32);

    // 视频状态区
    create_label(hwnd, "播放状态:", 20, 360, 100, 20, 0, false);
    create_label(hwnd, "未加载视频壁纸", 100, 360, 350, 20, IDC_VIDEO_STATUS, false);
    create_label(hwnd, "当前文件:", 20, 385, 100, 20, 0, false);
    create_label(hwnd, "-", 100, 385, 350, 20, IDC_CURRENT_FILE, false);

    // 进度条
    create_progress(hwnd, 20, 410, 420, 16, IDC_VIDEO_PROGRESS);

    // 关闭按钮
    create_button(hwnd, "关闭", 350, 445, 90, 32, IDC_BTN_CLOSE, BS_PUSHBUTTON as u32);
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORDLG => {
            let dark = DARK_MODE.with(|d| *d.borrow());
            let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut _);
            let text_color = if dark { 0x00FFFFFF } else { 0x00000000 };
            let bg_color = if dark { 0x00202020 } else { 0x00F3F3F3 };
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLORREF(text_color));
            SetBkColor(hdc, COLORREF(bg_color));
            let brush = CreateSolidBrush(COLORREF(bg_color));
            LRESULT(brush.0 as isize)
        }
        WM_COMMAND => {
            let cmd = (wparam.0 as u32) & 0xFFFF;
            let target = {
                let parent = GetParent(hwnd);
                if let Ok(p) = parent {
                    if !p.0.is_null() { p } else { HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(ptr::null_mut()))) }
                } else {
                    HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(ptr::null_mut())))
                }
            };
            match cmd as u16 {
                IDC_BTN_CLOSE => { let _ = DestroyWindow(hwnd); }
                IDC_BTN_PAUSE => { let _ = PostMessageW(Some(target), WM_COMMAND, WPARAM(CMD_PAUSE_TOGGLE), LPARAM(0)); }
                IDC_BTN_SELECT_IMAGE => { let _ = PostMessageW(Some(target), WM_COMMAND, WPARAM(CMD_SELECT_IMAGE), LPARAM(0)); }
                IDC_BTN_SELECT_VIDEO => { let _ = PostMessageW(Some(target), WM_COMMAND, WPARAM(CMD_SELECT_VIDEO), LPARAM(0)); }
                IDC_BTN_SELECT_PACKAGE => { let _ = PostMessageW(Some(target), WM_COMMAND, WPARAM(CMD_SELECT_PACKAGE), LPARAM(0)); }
                IDC_BTN_OPEN_AUDIO => { let _ = PostMessageW(Some(target), WM_COMMAND, WPARAM(CMD_OPEN_AUDIO), LPARAM(0)); }
                IDC_CARD_AURORA | IDC_CARD_PARTICLES | IDC_CARD_IMAGE | IDC_CARD_VIDEO => {
                    let _ = PostMessageW(Some(target), WM_COMMAND, WPARAM(CMD_WALLPAPER_CHANGED), LPARAM(cmd as isize));
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_HSCROLL => {
            let slider = GetDlgItem(Some(hwnd), IDC_VOLUME_SLIDER as i32);
            if let Ok(s) = slider {
                if !s.0.is_null() {
                    let pos = SendMessageW(s, TBM_GETPOS, Some(WPARAM(0)), Some(LPARAM(0)));
                    let vol = pos.0.max(0).min(100) as u32;
                    CURRENT_VOLUME.with(|v| *v.borrow_mut() = vol);
                    let label = GetDlgItem(Some(hwnd), IDC_VOLUME_LABEL as i32);
                    if let Ok(lbl) = label {
                        if !lbl.0.is_null() {
                            let text = format!("{}%\0", vol);
                            let wide: Vec<u16> = text.encode_utf16().collect();
                            let _ = SendMessageW(lbl, WM_SETTEXT, Some(WPARAM(0)), Some(LPARAM(wide.as_ptr() as isize)));
                        }
                    }
                    let target = {
                        let parent = GetParent(hwnd);
                        if let Ok(p) = parent {
                            if !p.0.is_null() { p } else { HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(ptr::null_mut()))) }
                        } else {
                            HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(ptr::null_mut())))
                        }
                    };
                    let _ = PostMessageW(Some(target), WM_COMMAND, WPARAM(CMD_VOLUME_CHANGED), LPARAM(vol as isize));
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let key = wparam.0 as u16;
            match key {
                VK_ESCAPE => { let _ = DestroyWindow(hwnd); LRESULT(0) }
                VK_SPACE => {
                    let target = {
                        let parent = GetParent(hwnd);
                        if let Ok(p) = parent {
                            if !p.0.is_null() { p } else { HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(ptr::null_mut()))) }
                        } else {
                            HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(ptr::null_mut())))
                        }
                    };
                    let _ = PostMessageW(Some(target), WM_COMMAND, WPARAM(CMD_PAUSE_TOGGLE), LPARAM(0));
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        WM_CLOSE | WM_DESTROY => {
            SETTINGS_HWND.with(|s| *s.borrow_mut() = None);
            HIDDEN_HWND.with(|h| {
                let hidden = h.borrow().unwrap_or(HWND(ptr::null_mut()));
                if !hidden.0.is_null() {
                    let _ = PostMessageW(Some(hidden), WM_COMMAND, WPARAM(CMD_SETTINGS_CLOSED), LPARAM(0));
                }
            });
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub fn update_video_status(hwnd: HWND, text: &str) {
    unsafe {
        if hwnd.0.is_null() { return; }
        let lbl = GetDlgItem(Some(hwnd), IDC_VIDEO_STATUS as i32);
        if let Ok(l) = lbl {
            if !l.0.is_null() {
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = SendMessageW(l, WM_SETTEXT, Some(WPARAM(0)), Some(LPARAM(wide.as_ptr() as isize)));
            }
        }
    }
}

pub fn update_video_progress(hwnd: HWND, progress: f32) {
    unsafe {
        if hwnd.0.is_null() { return; }
        let pb = GetDlgItem(Some(hwnd), IDC_VIDEO_PROGRESS as i32);
        if let Ok(p) = pb {
            if !p.0.is_null() {
                let _ = SendMessageW(p, PBM_SETRANGE32, Some(WPARAM(0)), Some(LPARAM((100isize) << 16)));
                let _ = SendMessageW(p, PBM_SETPOS, Some(WPARAM((progress * 100.0) as usize)), Some(LPARAM(0)));
            }
        }
    }
}

pub fn update_wallpaper_selection(hwnd: HWND, _wp_id: u32) {
    unsafe {
        if hwnd.0.is_null() { return; }
        // 简化处理：不做视觉高亮，只保证功能
    }
}

pub fn update_autostart_check(hwnd: HWND, enabled: bool) {
    unsafe {
        if hwnd.0.is_null() { return; }
        let cb = GetDlgItem(Some(hwnd), IDC_CHECK_AUTOSTART as i32);
        if let Ok(c) = cb {
            if !c.0.is_null() {
                let check = if enabled { BST_CHECKED } else { 0 };
                let _ = SendMessageW(c, BM_SETCHECK, Some(WPARAM(check)), Some(LPARAM(0)));
            }
        }
    }
}

pub fn update_current_file(hwnd: HWND, path: &str) {
    unsafe {
        if hwnd.0.is_null() { return; }
        let lbl = GetDlgItem(Some(hwnd), IDC_CURRENT_FILE as i32);
        if let Ok(l) = lbl {
            if !l.0.is_null() {
                let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = SendMessageW(l, WM_SETTEXT, Some(WPARAM(0)), Some(LPARAM(wide.as_ptr() as isize)));
            }
        }
    }
}

pub fn update_pause_button(_hwnd: HWND, _paused: bool) {
    // 简化处理
}

pub fn update_audio_label(_hwnd: HWND, _name: &str) {
    // 简化处理
}

pub fn query_autostart_check(hwnd: HWND) -> bool {
    unsafe {
        if hwnd.0.is_null() { return false; }
        let cb = GetDlgItem(Some(hwnd), IDC_CHECK_AUTOSTART as i32);
        if let Ok(c) = cb {
            if !c.0.is_null() {
                // 简化：默认返回 false，不做实际查询
                return false;
            }
        }
        false
    }
}
