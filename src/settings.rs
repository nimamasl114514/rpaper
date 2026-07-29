//! 设置界面 — Win32 原生窗口（Win11 设置面板风格）
//!
//! UI 设计:
//! - 圆角窗口（DWMWCP_ROUND）+ Mica 亮色背景 RGB(243,243,243)
//! - 白色卡片分层：4 张纯白卡片浮于浅灰背景上，无 GroupBox 边框
//! - 卡片标题：微软雅黑 UI 13pt 加粗，顶部居左，靠字体/间距做视觉层次
//! - 正文：微软雅黑 UI 9pt 常规，主文字深灰 RGB(50,50,50)，次要中灰 RGB(102,102,102)
//! - 控件：进度条平滑、按钮扁平化、单选/复选框无 3D 边框
//! - 自定义蓝紫图标嵌入窗口类和托盘
//! - Esc 关闭 / Space 暂停 / F5 刷新状态
//! - 按钮带 tooltip 悬浮提示

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM, LRESULT, COLORREF, RECT, HMODULE};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, HBRUSH, CreateSolidBrush,
    FW_NORMAL, FW_BOLD, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS,
    CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, DEFAULT_PITCH,
    TRANSPARENT, SetBkMode, SetTextColor,
    FillRect, BeginPaint, EndPaint, PAINTSTRUCT,
};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassExW, ShowWindow, DestroyWindow,
    SendMessageW, PostMessageW, GetDlgItem, GetDlgCtrlID, GetParent, GetClientRect, LoadImageW,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW, HMENU,
    WS_OVERLAPPED, WS_CAPTION, WS_SYSMENU, WS_CHILD, WS_VISIBLE, WS_CLIPSIBLINGS,
    WM_CREATE, WM_COMMAND, WM_DESTROY, WM_HSCROLL, WM_SETTEXT,
    WM_ERASEBKGND, WM_CTLCOLORDLG, WM_PAINT, WM_CLOSE,
    WM_GETMINMAXINFO, MINMAXINFO, SW_SHOW,
    WM_CTLCOLORSTATIC, WM_CTLCOLORBTN,
    WM_KEYDOWN, WM_SETFONT,
    BS_PUSHBUTTON, BS_AUTORADIOBUTTON, BS_AUTOCHECKBOX, BS_FLAT,
    BM_GETCHECK, BM_SETCHECK, HICON,
    IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
};
use windows::Win32::UI::Controls::{
    INITCOMMONCONTROLSEX, ICC_BAR_CLASSES, ICC_PROGRESS_CLASS, InitCommonControlsEx,
    TBS_HORZ, PROGRESS_CLASSW,
    PBM_SETRANGE32, PBM_SETPOS, TOOLTIPS_CLASSW, SetWindowTheme,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use std::cell::RefCell;

// === raw 常量 ===
const VK_ESCAPE: u16 = 0x1B;
const VK_SPACE: u16 = 0x20;
const VK_F5: u16 = 0x74;
const BST_CHECKED: isize = 1;
const TTM_ADDTOOLW: u32 = 0x0432;
const TTM_SETMAXTIPWIDTH: u32 = 0x0418;
const TTF_SUBCLASS: u32 = 0x0010;
const PBS_SMOOTH: u32 = 0x01;
const SS_CENTERIMAGE: u32 = 0x00000200;
const SS_LEFT: u32 = 0x00000000;
/// MAKEINTRESOURCEW(1) — 资源 ID 1，对应 .rc 中的应用图标
const IDI_APP: PCWSTR = PCWSTR(1 as *const u16);
const TBM_SETRANGE: u32 = 0x0406;
const TBM_SETPOS: u32 = 0x0405;
const TBM_GETPOS: u32 = 0x0400;

// === 控件 ID ===
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
pub const IDC_BTN_OPEN_AUDIO: u16 = 1014;
pub const IDC_AUDIO_LABEL: u16 = 1015;
pub const IDC_CHECK_AUTOSTART: u16 = 1016;
pub const IDC_VIDEO_STATUS: u16 = 1019;
pub const IDC_VIDEO_PROGRESS: u16 = 1020;
pub const IDC_CURRENT_FILE: u16 = 1021;
/// 4 个卡片标题文字
pub const IDC_TITLE1: u16 = 1111;
pub const IDC_TITLE2: u16 = 1112;
pub const IDC_TITLE3: u16 = 1113;
pub const IDC_TITLE4: u16 = 1114;

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

// === Win11 配色 ===
/// Mica 亮色背景 RGB(243,243,243)
const BG_COLOR: u32 = 0x00F3_F3F3;
/// 卡片纯白 RGB(255,255,255)
const CARD_COLOR: u32 = 0x00FF_FFFF;
/// 主文字 RGB(50,50,50)
const TEXT_COLOR: u32 = 0x0032_3232;
/// 次要文字 RGB(102,102,102)
const SUBTEXT_COLOR: u32 = 0x0066_6666;
/// Win11 accent 蓝 RGB(0,120,212) — 留作 BS_OWNERDRAW 自绘按钮时使用
#[allow(dead_code)]
const ACCENT_COLOR: u32 = 0x00D4_7800;

// === 布局常量 ===
/// 窗口宽/高（包含标题栏/边框，客户区约 544×680）
const WIN_W: i32 = 580;
const WIN_H: i32 = 740;
/// 卡片外边距（距窗口边缘）
const MARGIN: i32 = 24;
/// 卡片间距
const CARD_GAP: i32 = 12;
/// 卡片内边距
const PAD: i32 = 20;

thread_local! {
    static HIDDEN_HWND: RefCell<Option<HWND>> = const { RefCell::new(None) };
    /// 正文字体 — 微软雅黑 UI 9pt
    static UI_FONT: RefCell<Option<isize>> = const { RefCell::new(None) };
    /// 卡片标题字体 — 微软雅黑 UI 13pt 加粗
    static TITLE_FONT: RefCell<Option<isize>> = const { RefCell::new(None) };
    /// 窗口背景画刷
    static BG_BRUSH: RefCell<Option<HBRUSH>> = const { RefCell::new(None) };
    /// 卡片白色画刷
    static CARD_BRUSH: RefCell<Option<HBRUSH>> = const { RefCell::new(None) };
    /// 应用图标句柄（大图标 32px + 小图标 16px）
    static APP_ICON: RefCell<Option<HICON>> = const { RefCell::new(None) };
    static APP_ICON_SM: RefCell<Option<HICON>> = const { RefCell::new(None) };
    /// tooltip
    static TOOLTIP_HWND: RefCell<Option<HWND>> = const { RefCell::new(None) };
    /// 卡片矩形（客户区坐标，用于 WM_ERASEBKGND 绘制白色卡片）
    static CARD_RECTS: RefCell<Vec<RECT>> = const { RefCell::new(Vec::new()) };
}

pub fn set_hidden_hwnd(hwnd: HWND) {
    HIDDEN_HWND.with(|h| *h.borrow_mut() = Some(hwnd));
}

/// 加载资源图标 — 供 tray.rs 和 main.rs 使用
/// size=0 → 默认大图标(32px)，size=16 → 小图标(16px)
pub fn load_app_icon(small: bool) -> HICON {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or(HMODULE(0));
        let cx = if small { 16 } else { 32 };
        let flags = LR_DEFAULTSIZE | LR_SHARED;
        LoadImageW(hinst, IDI_APP, IMAGE_ICON, cx, cx, flags)
            .map(|h| HICON(h.0))
            .unwrap_or(HICON(0))
    }
}

pub struct SettingsWindow {
    pub hwnd: HWND,
}

impl SettingsWindow {
    pub fn create(parent: HWND) -> Result<Self, String> {
        unsafe {
            let icc = INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_BAR_CLASSES | ICC_PROGRESS_CLASS,
            };
            let _ = InitCommonControlsEx(&icc);

            let hinst = GetModuleHandleW(None).map_err(|e| format!("{e}"))?;
            let class_name = w!("RpaperSettings");

            // 加载图标（大 32px 标题栏/任务栏 + 小 16px 标题栏左上角）
            let hicon = LoadImageW(hinst, IDI_APP, IMAGE_ICON, 32, 32, LR_DEFAULTSIZE | LR_SHARED)
                .map(|h| HICON(h.0))
                .unwrap_or(HICON(0));
            let hicon_sm = LoadImageW(hinst, IDI_APP, IMAGE_ICON, 16, 16, LR_DEFAULTSIZE | LR_SHARED)
                .map(|h| HICON(h.0))
                .unwrap_or(HICON(0));
            APP_ICON.with(|i| *i.borrow_mut() = Some(hicon));
            APP_ICON_SM.with(|i| *i.borrow_mut() = Some(hicon_sm));

            // 画刷 — 提前创建给类注册和 WM_PAINT 使用
            let bg_brush = CreateSolidBrush(COLORREF(BG_COLOR));
            let card_brush = CreateSolidBrush(COLORREF(CARD_COLOR));
            BG_BRUSH.with(|b| *b.borrow_mut() = Some(bg_brush));
            CARD_BRUSH.with(|b| *b.borrow_mut() = Some(card_brush));

            let wcex = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(settings_wnd_proc),
                hInstance: hinst.into(),
                hIcon: hicon,
                hIconSm: hicon_sm,
                hbrBackground: bg_brush, // 类背景 = Mica 灰；白色卡片在 WM_PAINT 中叠加绘制
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
                WIN_W, WIN_H,
                parent, HMENU(0), hinst, None,
            );

            if hwnd.0 == 0 {
                return Err("创建设置窗口失败".into());
            }

            // Win11 圆角窗口
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const _ as *const _,
                std::mem::size_of::<i32>() as u32,
            );

            Ok(Self { hwnd })
        }
    }

    pub fn show(&self) {
        unsafe { let _ = ShowWindow(self.hwnd, SW_SHOW); }
    }
}

/// 字体选择
enum UiFont {
    Body,
    Title,
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_ctrl(
    class: PCWSTR, text: &str, style: u32,
    x: i32, y: i32, w: i32, h: i32, id: u16, parent: HWND,
) -> HWND {
    create_ctrl_with_font(class, text, style, x, y, w, h, id, parent, UiFont::Body)
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_ctrl_with_font(
    class: PCWSTR, text: &str, style: u32,
    x: i32, y: i32, w: i32, h: i32, id: u16, parent: HWND, font: UiFont,
) -> HWND {
    let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let hinst = match GetModuleHandleW(None) {
        Ok(h) => h,
        Err(_) => return HWND(0),
    };
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0), class, PCWSTR(text_wide.as_ptr()),
        WINDOW_STYLE(style),
        x, y, w, h,
        parent, HMENU(id as isize), hinst, None,
    );
    let font_handle = match font {
        UiFont::Body => UI_FONT.with(|f| *f.borrow()),
        UiFont::Title => TITLE_FONT.with(|f| *f.borrow()),
    };
    if let Some(f) = font_handle {
        let _ = SendMessageW(hwnd, WM_SETFONT, WPARAM(f as usize), LPARAM(1));
    }
    hwnd
}

/// 创建卡片标题文字 STATIC — 加粗 Title 字体，透明背景
unsafe fn create_card_title(text: &str, x: i32, y: i32, w: i32, id: u16, parent: HWND) -> HWND {
    let hwnd = create_ctrl_with_font(w!("STATIC"), text, SS_LEFT | WS_CHILD.0 | WS_VISIBLE.0,
        x, y, w, 24, id, parent, UiFont::Title);
    let _ = SetWindowTheme(hwnd, PCWSTR::null(), PCWSTR::null());
    hwnd
}

/// 创建正文 STATIC — 禁用 visual style，白色背景
unsafe fn create_static(text: &str, style: u32, x: i32, y: i32, w: i32, h: i32,
    id: u16, parent: HWND) -> HWND {
    let hwnd = create_ctrl(w!("STATIC"), text, style, x, y, w, h, id, parent);
    let _ = SetWindowTheme(hwnd, PCWSTR::null(), PCWSTR::null());
    hwnd
}

/// 添加 tooltip
unsafe fn add_tooltip(tooltip: HWND, ctrl: HWND, text: &str) {
    let text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    #[repr(C)]
    struct TOOLINFOW {
        cb_size: u32, flags: u32, hwnd: HWND, uid: usize,
        rect: RECT, hinst: isize, lpsz_text: *const u16, l_param: isize,
    }
    let ti = TOOLINFOW {
        cb_size: std::mem::size_of::<TOOLINFOW>() as u32,
        flags: TTF_SUBCLASS, hwnd: ctrl, uid: ctrl.0 as usize,
        rect: RECT::default(), hinst: 0, lpsz_text: text_wide.as_ptr(), l_param: 0,
    };
    let _ = SendMessageW(tooltip, TTM_ADDTOOLW, WPARAM(0), LPARAM(&ti as *const _ as isize));
}

extern "system" fn settings_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                let h = hwnd;

                // 字体
                let body_font = CreateFontW(
                    -14, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
                    DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
                    DEFAULT_PITCH.0 as u32, w!("Microsoft YaHei UI"),
                );
                UI_FONT.with(|f| *f.borrow_mut() = Some(body_font.0 as isize));

                let title_font = CreateFontW(
                    -17, 0, 0, 0, FW_BOLD.0 as i32, 0, 0, 0,
                    DEFAULT_CHARSET.0 as u32, OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32, CLEARTYPE_QUALITY.0 as u32,
                    DEFAULT_PITCH.0 as u32, w!("Microsoft YaHei UI"),
                );
                TITLE_FONT.with(|f| *f.borrow_mut() = Some(title_font.0 as isize));

                // tooltip
                let tooltip = CreateWindowExW(
                    WINDOW_EX_STYLE(0), TOOLTIPS_CLASSW, PCWSTR::null(),
                    WINDOW_STYLE(0x80000000),
                    CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT,
                    h, HMENU(0), GetModuleHandleW(None).unwrap_or_default(), None,
                );
                TOOLTIP_HWND.with(|t| *t.borrow_mut() = Some(tooltip));
                let _ = SendMessageW(tooltip, TTM_SETMAXTIPWIDTH, WPARAM(0), LPARAM(400));

                let cs: u32 = WS_CHILD.0 | WS_VISIBLE.0 | WS_CLIPSIBLINGS.0;
                let flat_btn = BS_PUSHBUTTON as u32 | BS_FLAT as u32;

                // ============================================================
                // 布局计算
                // 客户区约 544 宽，卡片宽 = 544 - 2*24 = 496
                // WM_PAINT 在 Mica 灰底上直接画 4 张白色矩形（无 WS_CLIPCHILDREN，可覆盖全区域）
                // 子控件通过 WM_CTLCOLOR* 返回白色画刷自绘背景，SetWindowTheme 禁用主题
                // ============================================================
                let cw = WIN_W - 16 - 20;
                let card_w = cw - 2 * MARGIN;
                let card_x = MARGIN;

                let mut card_rects = Vec::new();

                // --- 卡片1: 壁纸选择 ---
                let c1_y = MARGIN as i32;
                let c1_h = 192;
                card_rects.push(RECT { left: card_x, top: c1_y, right: card_x + card_w, bottom: c1_y + c1_h });
                create_card_title("壁纸选择", card_x + PAD, c1_y + 14, card_w - 2*PAD, IDC_TITLE1, h);
                // 单选按钮 2×2
                create_ctrl(w!("BUTTON"), "极光效果", BS_AUTORADIOBUTTON as u32 | cs,
                    card_x + PAD, c1_y + 50, 180, 28, IDC_RADIO_AURORA, h);
                create_ctrl(w!("BUTTON"), "粒子效果", BS_AUTORADIOBUTTON as u32 | cs,
                    card_x + PAD + 200, c1_y + 50, 180, 28, IDC_RADIO_PARTICLES, h);
                create_ctrl(w!("BUTTON"), "图片壁纸", BS_AUTORADIOBUTTON as u32 | cs,
                    card_x + PAD, c1_y + 84, 180, 28, IDC_RADIO_IMAGE, h);
                create_ctrl(w!("BUTTON"), "视频壁纸", BS_AUTORADIOBUTTON as u32 | cs,
                    card_x + PAD + 200, c1_y + 84, 180, 28, IDC_RADIO_VIDEO, h);
                // 3 个按钮
                let btn_w = (card_w - 2*PAD - 16) / 3;
                let btn_img = create_ctrl(w!("BUTTON"), "选择图片", flat_btn | cs,
                    card_x + PAD, c1_y + 132, btn_w, 36, IDC_BTN_SELECT_IMAGE, h);
                let btn_vid = create_ctrl(w!("BUTTON"), "选择视频", flat_btn | cs,
                    card_x + PAD + btn_w + 8, c1_y + 132, btn_w, 36, IDC_BTN_SELECT_VIDEO, h);
                let btn_pkg = create_ctrl(w!("BUTTON"), "加载壁纸包", flat_btn | cs,
                    card_x + PAD + 2*(btn_w + 8), c1_y + 132, btn_w, 36, IDC_BTN_SELECT_PACKAGE, h);

                // --- 卡片2: 音频 ---
                let c2_y = c1_y + c1_h + CARD_GAP;
                let c2_h = 112;
                card_rects.push(RECT { left: card_x, top: c2_y, right: card_x + card_w, bottom: c2_y + c2_h });
                create_card_title("音频", card_x + PAD, c2_y + 14, card_w - 2*PAD, IDC_TITLE2, h);
                create_static("音量: 50%", SS_LEFT | SS_CENTERIMAGE | cs,
                    card_x + PAD, c2_y + 52, 80, 24, IDC_VOLUME_LABEL, h);
                let slider_w = card_w - 2*PAD - 90;
                create_ctrl(w!("msctls_trackbar32"), "", TBS_HORZ | cs,
                    card_x + PAD + 90, c2_y + 50, slider_w, 28, IDC_VOLUME_SLIDER, h);
                let slider = GetDlgItem(h, IDC_VOLUME_SLIDER as i32);
                if slider.0 != 0 {
                    let _ = SetWindowTheme(slider, PCWSTR::null(), PCWSTR::null());
                    let _ = SendMessageW(slider, TBM_SETRANGE, WPARAM(1), LPARAM(100 << 16));
                    let _ = SendMessageW(slider, TBM_SETPOS, WPARAM(1), LPARAM(50));
                }
                let btn_audio = create_ctrl(w!("BUTTON"), "选择背景音乐", flat_btn | cs,
                    card_x + PAD, c2_y + 84, 140, 32, IDC_BTN_OPEN_AUDIO, h);
                create_static("未加载", SS_LEFT | SS_CENTERIMAGE | cs,
                    card_x + PAD + 150, c2_y + 88, card_w - 2*PAD - 150, 24, IDC_AUDIO_LABEL, h);

                // --- 卡片3: 视频状态 ---
                let c3_y = c2_y + c2_h + CARD_GAP;
                let c3_h = 168;
                card_rects.push(RECT { left: card_x, top: c3_y, right: card_x + card_w, bottom: c3_y + c3_h });
                create_card_title("视频状态", card_x + PAD, c3_y + 14, card_w - 2*PAD, IDC_TITLE3, h);
                create_static("未加载视频壁纸", SS_LEFT | cs,
                    card_x + PAD, c3_y + 50, card_w - 2*PAD, 48, IDC_VIDEO_STATUS, h);
                let prog_w = card_w - 2*PAD;
                create_ctrl(PROGRESS_CLASSW, "", PBS_SMOOTH | cs,
                    card_x + PAD, c3_y + 106, prog_w, 12, IDC_VIDEO_PROGRESS, h);
                let prog = GetDlgItem(h, IDC_VIDEO_PROGRESS as i32);
                if prog.0 != 0 {
                    let _ = SetWindowTheme(prog, PCWSTR::null(), PCWSTR::null());
                    let _ = SendMessageW(prog, PBM_SETRANGE32, WPARAM(0), LPARAM(100));
                    let _ = SendMessageW(prog, PBM_SETPOS, WPARAM(0), LPARAM(0));
                }
                create_static("当前文件: (无)", SS_LEFT | SS_CENTERIMAGE | cs,
                    card_x + PAD, c3_y + 128, prog_w, 24, IDC_CURRENT_FILE, h);

                // --- 卡片4: 通用 ---
                let c4_y = c3_y + c3_h + CARD_GAP;
                let c4_h = 84;
                card_rects.push(RECT { left: card_x, top: c4_y, right: card_x + card_w, bottom: c4_y + c4_h });
                create_card_title("通用", card_x + PAD, c4_y + 14, card_w - 2*PAD, IDC_TITLE4, h);
                create_ctrl(w!("BUTTON"), "开机自动启动", BS_AUTOCHECKBOX as u32 | cs,
                    card_x + PAD, c4_y + 50, 160, 28, IDC_CHECK_AUTOSTART, h);
                let btn_pause = create_ctrl(w!("BUTTON"), "暂停壁纸", flat_btn | cs,
                    card_x + card_w - PAD - 134, c4_y + 46, 134, 34, IDC_BTN_PAUSE, h);

                // --- 底部关闭按钮（在 Mica 背景上，不在卡片内）---
                let btn_close = create_ctrl_with_font(w!("BUTTON"), "关闭设置", flat_btn | cs,
                    card_x + card_w - PAD - 110, c4_y + c4_h + 20, 110, 38,
                    IDC_BTN_CLOSE, h, UiFont::Title);

                // === 所有按钮/控件禁用视觉主题，确保 WM_CTLCOLOR* 白色背景生效 ===
                for &bid in &[IDC_RADIO_AURORA, IDC_RADIO_PARTICLES, IDC_RADIO_IMAGE, IDC_RADIO_VIDEO,
                              IDC_BTN_SELECT_IMAGE, IDC_BTN_SELECT_VIDEO, IDC_BTN_SELECT_PACKAGE,
                              IDC_BTN_OPEN_AUDIO, IDC_CHECK_AUTOSTART, IDC_BTN_PAUSE, IDC_BTN_CLOSE] {
                    let bh = GetDlgItem(h, bid as i32);
                    if bh.0 != 0 { let _ = SetWindowTheme(bh, PCWSTR::null(), PCWSTR::null()); }
                }

                // === tooltip ===
                TOOLTIP_HWND.with(|t| {
                    if let Some(tt) = t.borrow().as_ref() {
                        add_tooltip(*tt, btn_img, "选择本地图片作为壁纸 (png/jpg/bmp/webp/gif)");
                        add_tooltip(*tt, btn_vid, "选择本地视频作为壁纸 (mp4/mkv/avi/webm/mov)");
                        add_tooltip(*tt, btn_pkg, "加载 Rpaper 壁纸包 (.rwp) 或 Wallpaper Engine 包 (.pkg)");
                        add_tooltip(*tt, btn_audio, "选择背景音乐 (mp3/wav/ogg/flac)");
                        add_tooltip(*tt, btn_pause, "暂停/恢复壁纸动画 (Space)");
                        add_tooltip(*tt, btn_close, "关闭设置窗口 (Esc)");
                    }
                });

                // 存储卡片矩形（供调试/其他用途）
                CARD_RECTS.with(|r| *r.borrow_mut() = card_rects);

                // 更新 WM_GETMINMAXINFO 的固定尺寸
                // 窗口大小要调整为 WIN_W x WIN_H
                LRESULT(0)
            }
            WM_CTLCOLORSTATIC => {
                let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as isize);
                let ctrl_id = GetDlgCtrlID(HWND(lparam.0)) as u16;
                // 卡片标题文字用主色，状态/路径次要文字用中灰
                let text_color = match ctrl_id {
                    IDC_AUDIO_LABEL | IDC_CURRENT_FILE => SUBTEXT_COLOR,
                    _ => TEXT_COLOR,
                };
                if hdc.0 != 0 {
                    let _ = SetBkMode(hdc, TRANSPARENT);
                    let _ = SetTextColor(hdc, COLORREF(text_color));
                }
                // 白色卡片背景（卡片 STATIC、标题、正文文字都返回白色画刷）
                CARD_BRUSH.with(|b| LRESULT(b.borrow().unwrap_or(HBRUSH(0)).0 as isize))
            }
            WM_CTLCOLORDLG => {
                // 对话框背景 → Mica 灰（卡片之间的间隙显示此颜色）
                BG_BRUSH.with(|b| LRESULT(b.borrow().unwrap_or(HBRUSH(0)).0 as isize))
            }
            WM_ERASEBKGND => {
                // 用类背景刷填充整个客户区（Mica 灰）
                let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as isize);
                if hdc.0 != 0 {
                    BG_BRUSH.with(|br| {
                        if let Some(bg_br) = *br.borrow() {
                            let mut rc = RECT::default();
                            let _ = GetClientRect(hwnd, &mut rc);
                            let _ = FillRect(hdc, &rc, bg_br);
                        }
                    });
                }
                LRESULT(1) // 已擦除背景
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                if hdc.0 != 0 {
                    // 绘制 4 张白色卡片（在 Mica 灰背景上）
                    CARD_BRUSH.with(|br| {
                        if let Some(card_br) = *br.borrow() {
                            CARD_RECTS.with(|rects| {
                                for rc in rects.borrow().iter() {
                                    let _ = FillRect(hdc, rc, card_br);
                                }
                            });
                        }
                    });
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_CTLCOLORBTN => {
                // 单选/复选/按钮背景 → 白色（与卡片融合）
                CARD_BRUSH.with(|b| LRESULT(b.borrow().unwrap_or(HBRUSH(0)).0 as isize))
            }
            WM_KEYDOWN => {
                let key = wparam.0 as u16;
                let target = {
                    let parent = GetParent(hwnd);
                    if parent.0 != 0 { parent } else { HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(0))) }
                };
                match key {
                    VK_ESCAPE => { let _ = DestroyWindow(hwnd); LRESULT(0) }
                    VK_SPACE => { let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_PAUSE_TOGGLE), LPARAM(0)); LRESULT(0) }
                    VK_F5 => { crate::refresh_video_status(); LRESULT(0) }
                    _ => DefWindowProcW(hwnd, msg, wparam, lparam),
                }
            }
            WM_COMMAND => {
                let cmd = (wparam.0 as u32) & 0xFFFF;
                let target = {
                    let parent = GetParent(hwnd);
                    if parent.0 != 0 { parent } else { HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(0))) }
                };
                match cmd as u16 {
                    IDC_BTN_CLOSE => { let _ = DestroyWindow(hwnd); }
                    IDC_RADIO_AURORA | IDC_RADIO_PARTICLES | IDC_RADIO_IMAGE | IDC_RADIO_VIDEO => {
                        let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_WALLPAPER_CHANGED), LPARAM(cmd as isize));
                    }
                    IDC_BTN_SELECT_IMAGE => { let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_SELECT_IMAGE), LPARAM(0)); }
                    IDC_BTN_SELECT_VIDEO => { let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_SELECT_VIDEO), LPARAM(0)); }
                    IDC_BTN_SELECT_PACKAGE => { let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_SELECT_PACKAGE), LPARAM(0)); }
                    IDC_BTN_OPEN_AUDIO => { let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_OPEN_AUDIO), LPARAM(0)); }
                    IDC_BTN_PAUSE => { let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_PAUSE_TOGGLE), LPARAM(0)); }
                    IDC_CHECK_AUTOSTART => { let _ = PostMessageW(target, WM_COMMAND, WPARAM(CMD_AUTOSTART_TOGGLE), LPARAM(0)); }
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
                    (*mmi).ptMinTrackSize.x = WIN_W;
                    (*mmi).ptMinTrackSize.y = WIN_H;
                    (*mmi).ptMaxTrackSize.x = WIN_W;
                    (*mmi).ptMaxTrackSize.y = WIN_H;
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                // 点击右上角 X 或发送 WM_CLOSE 时销毁窗口
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                HIDDEN_HWND.with(|h| {
                    let hidden = h.borrow().unwrap_or(HWND(0));
                    if hidden.0 != 0 {
                        let _ = PostMessageW(hidden, WM_COMMAND, WPARAM(CMD_SETTINGS_CLOSED), LPARAM(0));
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
            0 => IDC_RADIO_AURORA, 1 => IDC_RADIO_PARTICLES,
            2 => IDC_RADIO_IMAGE, 3 => IDC_RADIO_VIDEO,
            _ => return,
        };
        for &id in &[IDC_RADIO_AURORA, IDC_RADIO_PARTICLES, IDC_RADIO_IMAGE, IDC_RADIO_VIDEO] {
            let h = GetDlgItem(hwnd, id as i32);
            if h.0 != 0 {
                let _ = SendMessageW(h, BM_SETCHECK, WPARAM(if id == radio { 1 } else { 0 }), LPARAM(0));
            }
        }
    }
}

pub fn update_audio_label(hwnd: HWND, text: &str) {
    unsafe {
        let full = format!("{}\0", text);
        let wide: Vec<u16> = full.encode_utf16().collect();
        let h = GetDlgItem(hwnd, IDC_AUDIO_LABEL as i32);
        if h.0 != 0 { let _ = SendMessageW(h, WM_SETTEXT, WPARAM(0), LPARAM(wide.as_ptr() as isize)); }
    }
}

pub fn update_pause_button(hwnd: HWND, paused: bool) {
    unsafe {
        let text = if paused { "恢复壁纸\0" } else { "暂停壁纸\0" };
        let wide: Vec<u16> = text.encode_utf16().collect();
        let h = GetDlgItem(hwnd, IDC_BTN_PAUSE as i32);
        if h.0 != 0 { let _ = SendMessageW(h, WM_SETTEXT, WPARAM(0), LPARAM(wide.as_ptr() as isize)); }
    }
}

pub fn query_autostart_check(hwnd: HWND) -> bool {
    unsafe {
        let h = GetDlgItem(hwnd, IDC_CHECK_AUTOSTART as i32);
        if h.0 == 0 { return false; }
        SendMessageW(h, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == BST_CHECKED as isize
    }
}

pub fn update_autostart_check(hwnd: HWND, checked: bool) {
    unsafe {
        let h = GetDlgItem(hwnd, IDC_CHECK_AUTOSTART as i32);
        if h.0 != 0 {
            let _ = SendMessageW(h, BM_SETCHECK,
                WPARAM(if checked { BST_CHECKED as usize } else { 0 }), LPARAM(0));
        }
    }
}

pub fn update_video_status(hwnd: HWND, text: &str) {
    unsafe {
        let full = format!("{}\0", text);
        let wide: Vec<u16> = full.encode_utf16().collect();
        let h = GetDlgItem(hwnd, IDC_VIDEO_STATUS as i32);
        if h.0 != 0 { let _ = SendMessageW(h, WM_SETTEXT, WPARAM(0), LPARAM(wide.as_ptr() as isize)); }
    }
}

pub fn update_video_progress(hwnd: HWND, percent: u32) {
    unsafe {
        let h = GetDlgItem(hwnd, IDC_VIDEO_PROGRESS as i32);
        if h.0 != 0 { let _ = SendMessageW(h, PBM_SETPOS, WPARAM(percent as usize), LPARAM(0)); }
    }
}

pub fn update_current_file(hwnd: HWND, path: &str) {
    unsafe {
        let full = format!("当前文件: {}\0", path);
        let wide: Vec<u16> = full.encode_utf16().collect();
        let h = GetDlgItem(hwnd, IDC_CURRENT_FILE as i32);
        if h.0 != 0 { let _ = SendMessageW(h, WM_SETTEXT, WPARAM(0), LPARAM(wide.as_ptr() as isize)); }
    }
}
