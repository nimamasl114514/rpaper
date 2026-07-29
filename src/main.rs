//! Rpaper - 动态壁纸引擎入口
//!
//! windows subsystem 不会弹出 cmd 窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod config;
mod desktop;
mod error;
mod pkg;
mod rwp;
mod settings;
mod sys_wallpaper;
mod tray;
mod video;
mod wallpaper;
mod wallpapers;

use app::{App, PkgStatus, WallpaperType};
use config::AppConfig;
use std::path::PathBuf;
use std::cell::RefCell;
use tray::*;
use settings::*;
use crate::video::decoder::DecoderState;
use windows::Win32::Foundation::{HWND, HINSTANCE, LPARAM, WPARAM, LRESULT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, PeekMessageW,
    PostQuitMessage, RegisterClassExW, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    WINDOW_EX_STYLE, WM_COMMAND, WM_COPYDATA, WM_DESTROY, WM_QUIT, WM_TIMER,
    WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
    SetProcessDPIAware, MessageBoxW, MB_OK, MB_ICONERROR,
    SetTimer, KillTimer, FindWindowW, SendMessageW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OPENFILENAMEW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::core::{w, PCWSTR};
use windows::Win32::UI::WindowsAndMessaging::PM_REMOVE;
use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
use windows::Win32::UI::Shell::SHChangeNotify;
use windows::Win32::UI::Shell::SHCNE_ASSOCCHANGED;
use windows::Win32::UI::Shell::SHCNF_IDLIST;

const IMAGE_FILTER: &str = "图片\0*.png;*.jpg;*.jpeg;*.bmp;*.webp;*.gif\0所有文件\0*.*\0";
const VIDEO_FILTER: &str = "视频\0*.mp4;*.mkv;*.avi;*.webm;*.mov;*.flv;*.wmv;*.ts;*.m4v\0所有文件\0*.*\0";
const AUDIO_FILTER: &str = "音频文件\0*.mp3;*.wav;*.ogg;*.flac;*.m4a;*.aac\0所有文件\0*.*\0";
const RWP_FILTER: &str = "Rpaper 壁纸包\0*.rwp\0所有文件\0*.*\0";

/// 托盘左键单击/双击去抖计时器 ID
const IDM_TRAY_CLICK_TIMER: usize = 9001;
/// 视频状态刷新定时器 ID — 1 秒一次，仅在设置窗口打开时刷新
const IDM_VIDEO_STATUS_TIMER: usize = 9002;

/// 单实例互斥体名 — Local\ 前缀仅当前会话可见，不跨用户会话
const SINGLE_INSTANCE_MUTEX: &str = "Local\\Rpaper_SingleInstance_v1";
/// WM_COPYDATA 的 dwData 标识 — 区分自定义消息类型（'RPWP' 四字符码）
const COPYDATA_FILE_PATH: usize = 0x5250_5750;
/// Win32 ERROR_ALREADY_EXISTS 错误码（CreateMutexW 已存在时返回）
const ERROR_ALREADY_EXISTS_RAW: u32 = 183;

// windows 0.52 crate 的 GetLastError 返回 Result<(), Error>，CreateMutexW 包装也可能
// 在调用 FFI 后消耗 last error。用 raw FFI 直接绑定 kernel32，确保 FFI 调用后立即读
// GetLastError，中间无任何 Rust 代码干扰 last error 状态。
extern "system" {
    fn CreateMutexW(
        lpmutexattributes: *const std::ffi::c_void,
        binitialowner: i32,
        lpname: *const u16,
    ) -> *mut std::ffi::c_void;
    fn GetLastError() -> u32;
}

/// 尝试获取单实例所有权。
/// - 返回 true：当前是首个实例，应继续启动；互斥体 handle 隐式 leak 直到进程退出
/// - 返回 false：已有实例在运行，调用方应转发参数后立即退出
fn acquire_single_instance() -> bool {
    unsafe {
        let name_w: Vec<u16> = SINGLE_INSTANCE_MUTEX
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // raw FFI CreateMutexW 成功返回非空 handle，已存在时也返回有效 handle 但 GetLastError=183
        let _ = CreateMutexW(std::ptr::null(), 0, name_w.as_ptr());
        GetLastError() != ERROR_ALREADY_EXISTS_RAW
    }
}

/// 把命令行文件路径通过 WM_COPYDATA 转发给已运行实例的隐藏窗口。
/// SendMessageW 同步调用，返回时接收方已处理完毕，path_w 生命周期安全。
/// 返回 true 表示找到窗口并转发成功。
fn forward_file_path(path: &std::path::Path) -> bool {
    unsafe {
        // 隐藏窗口类名 WallpaperMsg — 由 create_hidden_window 注册
        let hwnd = FindWindowW(w!("WallpaperMsg"), PCWSTR::null());
        if hwnd.0 == 0 {
            return false;
        }

        // UTF-16 路径 + null terminator
        let path_w: Vec<u16> = path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let byte_len = path_w.len() * 2;

        let cds = COPYDATASTRUCT {
            dwData: COPYDATA_FILE_PATH,
            cbData: byte_len as u32,
            lpData: path_w.as_ptr() as *mut _,
        };

        // SendMessageW 同步 — 接收方处理完才返回，期间 path_w 必须存活（栈上有效）
        let _ = SendMessageW(
            hwnd,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&cds as *const _ as isize),
        );
        true
    }
}

/// 托盘左键单击切换壁纸 — 在 Aurora / Particles / Image / Video 间循环
fn tray_left_click_action() {
    APP.with(|a| {
        if let Some(app) = &mut *a.borrow_mut() {
            let current = app.current_wallpaper();
            let next = match current {
                WallpaperType::Aurora => WallpaperType::Particles,
                WallpaperType::Particles => {
                    if app.has_image() { WallpaperType::Image } else { WallpaperType::Aurora }
                }
                WallpaperType::Image => {
                    if app.has_video() { WallpaperType::Video } else { WallpaperType::Aurora }
                }
                WallpaperType::Video => WallpaperType::Aurora,
            };
            app.switch_wallpaper(next);
            let wp_str = match next {
                WallpaperType::Aurora => "aurora",
                WallpaperType::Particles => "particles",
                WallpaperType::Image => "image",
                WallpaperType::Video => "video",
            };
            CONFIG.with(|c| {
                let mut cfg = c.borrow_mut();
                cfg.wallpaper_type = wp_str.into();
                let _ = cfg.save();
            });
        }
    });
}

/// 刷新设置窗口中的视频状态文本 — 由 1 秒定时器触发
/// 优先级: .pkg 解压状态 > 视频解码状态 > 未加载提示
/// 设置窗口未打开时直接返回，省一次锁
fn refresh_video_status() {
    let settings_hwnd = SETTINGS_HWND.with(|s| s.borrow().unwrap_or(HWND(0)));
    if settings_hwnd.0 == 0 { return; }

    let (text, progress) = APP.with(|a| {
        let borrow = a.borrow();
        let Some(app) = borrow.as_ref() else { return ("未加载视频壁纸".to_string(), 0u32); };
        // 优先检查 .pkg 解压状态 — 解压中/失败时不进视频状态分支
        match app.pkg_status() {
            Some(PkgStatus::Extracting) => return ("正在解压 Wallpaper Engine .pkg 包...".to_string(), 0),
            Some(PkgStatus::Failed(e)) => return (format!("解压失败: {e}"), 0),
            None => {}
        }
        match app.current_wallpaper() {
            WallpaperType::Video => match app.video_status() {
                Some(st) => {
                    let state_text = match st.state {
                        DecoderState::Loading => "加载中（首帧未出）",
                        DecoderState::Playing => "播放中",
                        DecoderState::Error => "解码错误",
                    };
                    // 用 match 而非 `if total > 0` 避免 clippy::manual_checked_ops
                    let pct = match st.total {
                        0 => 0,
                        total => (st.current * 100 / total).min(100),
                    };
                    let text = format!(
                        "状态: {}\r\n进度: {}%  ({}/{})\r\n视频尺寸: {}×{}",
                        state_text, pct, st.current, st.total,
                        app.video_width(), app.video_height()
                    );
                    (text, pct as u32)
                }
                None => ("视频未初始化".to_string(), 0),
            },
            _ => ("未加载视频壁纸".to_string(), 0),
        }
    });
    update_video_status(settings_hwnd, &text);
    update_video_progress(settings_hwnd, progress);
}

/// 打开设置窗口（已打开则前置）
fn open_settings_window() {
    SETTINGS_HWND.with(|s| {
        let existing = s.borrow().unwrap_or(HWND(0));
        if existing.0 != 0 {
            unsafe { let _ = SetForegroundWindow(existing); }
            return;
        }
        match SettingsWindow::create(HWND(0)) {
            Ok(win) => {
                win.show();
                unsafe { let _ = SetForegroundWindow(win.hwnd); }
                let wp_id = APP.with(|a| {
                    a.borrow().as_ref()
                        .map(|app| app.current_wallpaper_id())
                        .unwrap_or(0)
                });
                update_wallpaper_selection(win.hwnd, wp_id);
                let autostart = CONFIG.with(|c| c.borrow().autostart);
                update_autostart_check(win.hwnd, autostart);
                SETTINGS_HWND.with(|s| *s.borrow_mut() = Some(win.hwnd));
                // 打开即刷新一次视频状态，避免空白 1 秒
                refresh_video_status();
            }
            Err(e) => show_error(&e),
        }
    });
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
    static HIDDEN_HWND: RefCell<Option<HWND>> = const { RefCell::new(None) };
    static SETTINGS_HWND: RefCell<Option<HWND>> = const { RefCell::new(None) };
    static CONFIG: RefCell<AppConfig> = RefCell::new(AppConfig::default());
    /// 延迟弹窗错误槽 — WM_COPYDATA 处理期间不能调 MessageBoxW（会阻塞发送方进程）
    /// 错误先存这里，主循环释放 APP borrow 后取出弹窗
    static PENDING_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// 启动目标 — 命令行文件路径优先，否则用关键字/配置决定壁纸类型
enum StartupTarget {
    Wallpaper(WallpaperType),
    File(PathBuf),
}

fn default_wp_from_config(config: &AppConfig) -> WallpaperType {
    match config.wallpaper_type.as_str() {
        "particles" => WallpaperType::Particles,
        "image" => WallpaperType::Image,
        "video" => WallpaperType::Video,
        _ => WallpaperType::Aurora,
    }
}

/// 解析命令行参数: 文件路径 > 关键字 > 配置默认值
fn parse_args(config: &AppConfig) -> StartupTarget {
    if let Some(arg1) = std::env::args().nth(1) {
        // 优先：已存在的文件路径
        let path = PathBuf::from(&arg1);
        if path.is_file() {
            return StartupTarget::File(path);
        }
        // 关键字
        return match arg1.as_str() {
            "aurora" | "a" => StartupTarget::Wallpaper(WallpaperType::Aurora),
            "particles" | "particle" | "p" => StartupTarget::Wallpaper(WallpaperType::Particles),
            "image" | "i" => StartupTarget::Wallpaper(WallpaperType::Image),
            "video" | "v" => StartupTarget::Wallpaper(WallpaperType::Video),
            _ => StartupTarget::Wallpaper(default_wp_from_config(config)),
        };
    }
    StartupTarget::Wallpaper(default_wp_from_config(config))
}

fn open_file_dialog(filter: &str, title: &str) -> Option<PathBuf> {
    let hwnd = HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(0)));
    unsafe {
        let mut file_buf = [0u16; 260];
        let filter: Vec<u16> = filter.encode_utf16().collect();
        let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();

        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: hwnd,
            hInstance: HINSTANCE(0),
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrCustomFilter: windows::core::PWSTR::null(),
            nMaxCustFilter: 0, nFilterIndex: 1,
            lpstrFile: windows::core::PWSTR(file_buf.as_mut_ptr()),
            nMaxFile: 260,
            lpstrFileTitle: windows::core::PWSTR::null(), nMaxFileTitle: 0,
            lpstrInitialDir: PCWSTR::null(),
            lpstrTitle: PCWSTR(title_wide.as_ptr()),
            Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
            nFileOffset: 0, nFileExtension: 0, lpstrDefExt: PCWSTR::null(),
            lCustData: LPARAM(0), lpfnHook: None, lpTemplateName: PCWSTR::null(),
            pvReserved: std::ptr::null_mut(), dwReserved: 0,
            FlagsEx: windows::Win32::UI::Controls::Dialogs::OPEN_FILENAME_FLAGS_EX(0),
        };

        if GetOpenFileNameW(&mut ofn).as_bool() {
            let len = file_buf.iter().position(|&c| c == 0).unwrap_or(0);
            Some(PathBuf::from(String::from_utf16_lossy(&file_buf[..len])))
        } else {
            None
        }
    }
}

fn show_error(msg: &str) {
    // 走 RpaperError 分类，输出友好提示
    let friendly = error::RpaperError::from_message(msg).to_string();
    let hwnd = HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(0)));
    let wstr: Vec<u16> = friendly.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "Rpaper 错误\0".encode_utf16().collect();
    unsafe {
        let _ = MessageBoxW(hwnd, PCWSTR(wstr.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR);
    }
}

/// 把 UTF-8 字符串转成以 0 结尾的 UTF-16 wide string
fn wide_z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 写 HKCU 下的字符串注册表值（子键不存在会自动创建）
fn reg_set_sz(subkey: &str, value_name: Option<&str>, value: &str) -> Result<(), String> {
    let subkey_w = wide_z(subkey);
    let value_w = wide_z(value);
    let name_w = value_name.map(wide_z);
    let name_ptr = match &name_w {
        Some(w) => PCWSTR(w.as_ptr()),
        None => PCWSTR::null(),
    };
    unsafe {
        windows::Win32::System::Registry::RegSetKeyValueW(
            windows::Win32::System::Registry::HKEY_CURRENT_USER,
            PCWSTR(subkey_w.as_ptr()),
            name_ptr,
            windows::Win32::System::Registry::REG_SZ.0,
            Some(value_w.as_ptr() as *const std::ffi::c_void),
            (value_w.len() * 2) as u32,
        ).map_err(|e| format!("RegSetKeyValueW({subkey}) 失败: {e}"))
    }
}

/// 注册文件关联到当前 exe（HKCU 级，无需 UAC）
/// - .rwp / .pkg → 双击直接用 Rpaper 打开，带自定义图标
/// - 视频/图片格式 → 右键菜单添加"用 Rpaper 设置为壁纸"
fn register_file_association() {
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return,
    };
    const PROGID_RWP: &str = "Rpaper.WallpaperPackage";
    const PROGID_PKG: &str = "Rpaper.WallpaperEnginePkg";

    // === .rwp 壁纸包 ===
    let _ = reg_set_sz("Software\\Classes\\.rwp", None, PROGID_RWP);
    let _ = reg_set_sz(&format!("Software\\Classes\\{PROGID_RWP}"), None, "Rpaper 壁纸包");
    let _ = reg_set_sz(&format!("Software\\Classes\\{PROGID_RWP}\\DefaultIcon"), None, &format!("{exe},0"));
    let _ = reg_set_sz(
        &format!("Software\\Classes\\{PROGID_RWP}\\shell\\open\\command"),
        None, &format!("\"{exe}\" \"%1\""),
    );

    // === .pkg (Wallpaper Engine) ===
    let _ = reg_set_sz("Software\\Classes\\.pkg", None, PROGID_PKG);
    let _ = reg_set_sz(&format!("Software\\Classes\\{PROGID_PKG}"), None, "Wallpaper Engine 壁纸包");
    let _ = reg_set_sz(&format!("Software\\Classes\\{PROGID_PKG}\\DefaultIcon"), None, &format!("{exe},0"));
    let _ = reg_set_sz(
        &format!("Software\\Classes\\{PROGID_PKG}\\shell\\open\\command"),
        None, &format!("\"{exe}\" \"%1\""),
    );

    // === 右键"用 Rpaper 设置为壁纸" — 给视频和图片格式添加 ===
    // 通过 *\shell 注册所有文件的右键菜单（Windows 会自动只在相关类型显示）
    let rpaper_shell = "Software\\Classes\\*\\shell\\RpaperSetWallpaper";
    let _ = reg_set_sz(rpaper_shell, None, "用 Rpaper 设置为壁纸");
    let _ = reg_set_sz(rpaper_shell, Some("Icon"), &format!("{exe},0"));
    let _ = reg_set_sz(
        &format!("{rpaper_shell}\\command"),
        None, &format!("\"{exe}\" \"%1\""),
    );

    // 通知 Explorer 刷新文件关联
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

/// 清理 %TEMP% 下 rpaper_pkg_* 残留临时文件
/// 上次崩溃/异常退出时 PkgVideo::drop 未触发会残留，启动时主动清理一遍
/// 命名前缀 rpaper_pkg_ 与 pkg.rs 中临时文件命名约定保持一致
fn cleanup_stale_pkg_temp() {
    let temp = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&temp) else { return; };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("rpaper_pkg_") {
            // 文件可能正被其他实例占用，删除失败直接忽略
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

extern "system" fn hidden_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TRAYICON => {
                let lparam_low = (lparam.0 as u32) & 0xFFFF;
                match lparam_low {
                    0x0205 /* WM_RBUTTONUP */ => {
                        TRAY.with(|t| {
                            if let Some(tray) = t.borrow().as_ref() {
                                let (wp_id, has_img, has_vid) = APP.with(|a| {
                                    a.borrow().as_ref()
                                        .map(|app| (app.current_wallpaper_id(), app.has_image(), app.has_video()))
                                        .unwrap_or((0, false, false))
                                });
                                tray.show_menu(wp_id, has_img, has_vid);
                            }
                        });
                        LRESULT(0)
                    }
                    0x0202 /* WM_LBUTTONUP */ => {
                        // 启动双击去抖计时器，期间若收到 WM_LBUTTONDBLCLK 则取消单击动作
                        let _ = SetTimer(hwnd, IDM_TRAY_CLICK_TIMER, GetDoubleClickTime(), None);
                        LRESULT(0)
                    }
                    0x0203 /* WM_LBUTTONDBLCLK */ => {
                        // 双击 → 取消单击计时器，打开设置窗口
                        let _ = KillTimer(hwnd, IDM_TRAY_CLICK_TIMER);
                        open_settings_window();
                        LRESULT(0)
                    }
                    _ => LRESULT(0),
                }
            }
            WM_COPYDATA => {
                // 单实例转发：第二个进程通过 WM_COPYDATA 把命令行文件路径发过来
                // 接收方必须以只读方式使用 lpData，SendMessageW 同步期间数据有效
                let cds_ptr = lparam.0 as *const COPYDATASTRUCT;
                if !cds_ptr.is_null() {
                    let cds = &*cds_ptr;
                    if cds.dwData == COPYDATA_FILE_PATH {
                        let wlen = (cds.cbData as usize) / 2;
                        let ptr = cds.lpData as *const u16;
                        let mut path_w: Vec<u16> = Vec::with_capacity(wlen);
                        for i in 0..wlen {
                            let c = *ptr.add(i);
                            if c == 0 { break; }
                            path_w.push(c);
                        }
                        if !path_w.is_empty() {
                            let path = PathBuf::from(String::from_utf16_lossy(&path_w));
                            // WM_COPYDATA 处理期间禁止调 MessageBoxW — 会阻塞发送方进程
                            // 用 silent_on_error=true 静默加载，失败时存到 PENDING_ERROR
                            // 主循环释放 APP borrow 后取 PENDING_ERROR 延迟弹窗
                            if let Err(e) = load_file_and_persist(path, true) {
                                PENDING_ERROR.with(|p| *p.borrow_mut() = Some(e));
                            }
                            // 设置窗口已打开时刷新壁纸类型单选
                            SETTINGS_HWND.with(|s| {
                                if let Some(hwnd) = s.borrow().as_ref() {
                                    let wp_id = APP.with(|a| {
                                        a.borrow().as_ref()
                                            .map(|app| app.current_wallpaper_id())
                                            .unwrap_or(0)
                                    });
                                    update_wallpaper_selection(*hwnd, wp_id);
                                    refresh_video_status();
                                }
                            });
                        }
                        return LRESULT(1); // 已处理
                    }
                }
                LRESULT(0)
            }
            WM_TIMER => {
                // 托盘左键单击延时到达 → 执行单击切换
                if wparam.0 == IDM_TRAY_CLICK_TIMER {
                    let _ = KillTimer(hwnd, IDM_TRAY_CLICK_TIMER);
                    tray_left_click_action();
                } else if wparam.0 == IDM_VIDEO_STATUS_TIMER {
                    // 1 秒一次刷新设置窗口视频状态文本
                    refresh_video_status();
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let cmd = (wparam.0 as u32) & 0xFFFF;
                match cmd as usize {
                    IDM_AURORA => {
                        APP.with(|a| { if let Some(app) = &mut *a.borrow_mut() { app.switch_wallpaper(WallpaperType::Aurora); } });
                        CONFIG.with(|c| {
                            let mut cfg = c.borrow_mut();
                            cfg.wallpaper_type = "aurora".into();
                            let _ = cfg.save();
                        });
                    }
                    IDM_PARTICLES => {
                        APP.with(|a| { if let Some(app) = &mut *a.borrow_mut() { app.switch_wallpaper(WallpaperType::Particles); } });
                        CONFIG.with(|c| {
                            let mut cfg = c.borrow_mut();
                            cfg.wallpaper_type = "particles".into();
                            let _ = cfg.save();
                        });
                    }
                    IDM_IMAGE => {
                        if let Some(path) = open_file_dialog(IMAGE_FILTER, "选择图片") {
                            let mut result_ok = false;
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    match app.load_file(path.clone()) {
                                        Ok(_) => { result_ok = true; }
                                        Err(e) => show_error(&e),
                                    }
                                }
                            });
                            if result_ok {
                                CONFIG.with(|c| {
                                    let mut cfg = c.borrow_mut();
                                    cfg.last_image_path = Some(path.to_string_lossy().to_string());
                                    cfg.wallpaper_type = "image".into();
                                    let _ = cfg.save();
                                });
                            }
                        }
                    }
                    IDM_VIDEO => {
                        if let Some(path) = open_file_dialog(VIDEO_FILTER, "选择视频") {
                            let mut result_ok = false;
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    match app.load_file(path.clone()) {
                                        Ok(_) => { result_ok = true; }
                                        Err(e) => show_error(&e),
                                    }
                                }
                            });
                            if result_ok {
                                CONFIG.with(|c| {
                                    let mut cfg = c.borrow_mut();
                                    cfg.last_video_path = Some(path.to_string_lossy().to_string());
                                    cfg.wallpaper_type = "video".into();
                                    let _ = cfg.save();
                                });
                            }
                        }
                    }
                    IDM_PACKAGE => {
                        if let Some(path) = open_file_dialog(RWP_FILTER, "加载壁纸包") {
                            let mut result_wp: Option<WallpaperType> = None;
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    match app.load_file(path.clone()) {
                                        Ok(wp) => { result_wp = Some(wp); }
                                        Err(e) => show_error(&e),
                                    }
                                }
                            });
                            if let Some(wp) = result_wp {
                                CONFIG.with(|c| {
                                    let mut cfg = c.borrow_mut();
                                    cfg.last_package_path = Some(path.to_string_lossy().to_string());
                                    cfg.wallpaper_type = match wp {
                                        WallpaperType::Aurora => "aurora",
                                        WallpaperType::Particles => "particles",
                                        WallpaperType::Image => "image",
                                        WallpaperType::Video => "video",
                                    }.into();
                                    let _ = cfg.save();
                                });
                            }
                        }
                    }
                    IDM_SETTINGS | CMD_OPEN_SETTINGS => {
                        // 打开设置窗口（如果已打开则前置）
                        SETTINGS_HWND.with(|s| {
                            let existing = s.borrow().unwrap_or(HWND(0));
                            if existing.0 != 0 {
                                let _ = SetForegroundWindow(existing);
                            } else {
                                match SettingsWindow::create(HWND(0)) {
                                    Ok(win) => {
                                        win.show();
                                        let _ = SetForegroundWindow(win.hwnd);
                                        let wp_id = APP.with(|a| {
                                            a.borrow().as_ref()
                                                .map(|app| app.current_wallpaper_id())
                                                .unwrap_or(0)
                                        });
                                        update_wallpaper_selection(win.hwnd, wp_id);
                                        // 初始化自启复选框状态
                                        let autostart = CONFIG.with(|c| c.borrow().autostart);
                                        update_autostart_check(win.hwnd, autostart);
                                        SETTINGS_HWND.with(|s| *s.borrow_mut() = Some(win.hwnd));
                                        // 打开即刷新视频状态
                                        refresh_video_status();
                                    }
                                    Err(e) => show_error(&e),
                                }
                            }
                        });
                    }
                    CMD_VOLUME_CHANGED => {
                        let vol = lparam.0 as f32 / 100.0;
                        APP.with(|a| { if let Some(app) = &mut *a.borrow_mut() { app.set_volume(vol); } });
                        CONFIG.with(|c| {
                            let mut cfg = c.borrow_mut();
                            cfg.volume = vol;
                            let _ = cfg.save();
                        });
                    }
                    CMD_WALLPAPER_CHANGED => {
                        let radio_id = lparam.0 as u16;
                        let wp = match radio_id {
                            IDC_RADIO_AURORA => WallpaperType::Aurora,
                            IDC_RADIO_PARTICLES => WallpaperType::Particles,
                            IDC_RADIO_IMAGE => WallpaperType::Image,
                            IDC_RADIO_VIDEO => WallpaperType::Video,
                            _ => return LRESULT(0),
                        };
                        APP.with(|a| { if let Some(app) = &mut *a.borrow_mut() { app.switch_wallpaper(wp); } });
                        CONFIG.with(|c| {
                            let mut cfg = c.borrow_mut();
                            cfg.wallpaper_type = match wp {
                                WallpaperType::Aurora => "aurora",
                                WallpaperType::Particles => "particles",
                                WallpaperType::Image => "image",
                                WallpaperType::Video => "video",
                            }.into();
                            let _ = cfg.save();
                        });
                    }
                    CMD_PAUSE_TOGGLE => {
                        APP.with(|a| {
                            if let Some(app) = &mut *a.borrow_mut() {
                                let paused = app.toggle_pause();
                                SETTINGS_HWND.with(|s| {
                                    if let Some(hwnd) = s.borrow().as_ref() {
                                        update_pause_button(*hwnd, paused);
                                    }
                                });
                            }
                        });
                    }
                    CMD_SELECT_IMAGE => {
                        if let Some(path) = open_file_dialog(IMAGE_FILTER, "选择图片") {
                            let mut result_ok = false;
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    match app.load_file(path.clone()) {
                                        Ok(_) => { result_ok = true; }
                                        Err(e) => show_error(&e),
                                    }
                                    SETTINGS_HWND.with(|s| {
                                        if let Some(hwnd) = s.borrow().as_ref() {
                                            update_wallpaper_selection(*hwnd, app.current_wallpaper_id());
                                        }
                                    });
                                }
                            });
                            if result_ok {
                                CONFIG.with(|c| {
                                    let mut cfg = c.borrow_mut();
                                    cfg.last_image_path = Some(path.to_string_lossy().to_string());
                                    cfg.wallpaper_type = "image".into();
                                    let _ = cfg.save();
                                });
                            }
                        }
                    }
                    CMD_SELECT_VIDEO => {
                        if let Some(path) = open_file_dialog(VIDEO_FILTER, "选择视频") {
                            let mut result_ok = false;
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    match app.load_file(path.clone()) {
                                        Ok(_) => { result_ok = true; }
                                        Err(e) => show_error(&e),
                                    }
                                    SETTINGS_HWND.with(|s| {
                                        if let Some(hwnd) = s.borrow().as_ref() {
                                            update_wallpaper_selection(*hwnd, app.current_wallpaper_id());
                                        }
                                    });
                                }
                            });
                            if result_ok {
                                CONFIG.with(|c| {
                                    let mut cfg = c.borrow_mut();
                                    cfg.last_video_path = Some(path.to_string_lossy().to_string());
                                    cfg.wallpaper_type = "video".into();
                                    let _ = cfg.save();
                                });
                            }
                        }
                    }
                    CMD_SELECT_PACKAGE => {
                        if let Some(path) = open_file_dialog(RWP_FILTER, "加载壁纸包") {
                            let mut result_wp: Option<WallpaperType> = None;
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    match app.load_file(path.clone()) {
                                        Ok(wp) => { result_wp = Some(wp); }
                                        Err(e) => show_error(&e),
                                    }
                                    SETTINGS_HWND.with(|s| {
                                        if let Some(hwnd) = s.borrow().as_ref() {
                                            update_wallpaper_selection(*hwnd, app.current_wallpaper_id());
                                        }
                                    });
                                }
                            });
                            if let Some(wp) = result_wp {
                                CONFIG.with(|c| {
                                    let mut cfg = c.borrow_mut();
                                    cfg.last_package_path = Some(path.to_string_lossy().to_string());
                                    cfg.wallpaper_type = match wp {
                                        WallpaperType::Aurora => "aurora",
                                        WallpaperType::Particles => "particles",
                                        WallpaperType::Image => "image",
                                        WallpaperType::Video => "video",
                                    }.into();
                                    let _ = cfg.save();
                                });
                            }
                        }
                    }
                    CMD_OPEN_AUDIO => {
                        if let Some(path) = open_file_dialog(AUDIO_FILTER, "选择背景音乐") {
                            let mut result_ok = false;
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    match app.load_audio_file(&path) {
                                        Ok(()) => {
                                            result_ok = true;
                                            let name = path.file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("已加载");
                                            SETTINGS_HWND.with(|s| {
                                                if let Some(hwnd) = s.borrow().as_ref() {
                                                    update_audio_label(*hwnd, name);
                                                }
                                            });
                                        }
                                        Err(e) => show_error(&e),
                                    }
                                }
                            });
                            if result_ok {
                                CONFIG.with(|c| {
                                    let mut cfg = c.borrow_mut();
                                    cfg.last_audio_path = Some(path.to_string_lossy().to_string());
                                    let _ = cfg.save();
                                });
                            }
                        }
                    }
                    CMD_AUTOSTART_TOGGLE => {
                        // 读取复选框状态决定写入或删除注册表
                        let checked = SETTINGS_HWND.with(|s| {
                            s.borrow().as_ref()
                                .map(|h| query_autostart_check(*h))
                                .unwrap_or(false)
                        });
                        let exe = std::env::current_exe().unwrap_or_default();
                        // RegSetKeyValueW 需要 UTF-16 数据，不能直接传 UTF-8 的 String
                        let exe_path_w: Vec<u16> = format!("\"{}\"", exe.to_string_lossy())
                            .encode_utf16()
                            .chain(std::iter::once(0))
                            .collect();

                        if checked {
                            // 写入注册表
                            let _ = windows::Win32::System::Registry::RegSetKeyValueW(
                                windows::Win32::System::Registry::HKEY_CURRENT_USER,
                                windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
                                windows::core::w!("Rpaper"),
                                windows::Win32::System::Registry::REG_SZ.0,
                                Some(exe_path_w.as_ptr() as *const _),
                                (exe_path_w.len() * 2) as u32,
                            );
                        } else {
                            // 删除注册表键
                            let _ = windows::Win32::System::Registry::RegDeleteKeyValueW(
                                windows::Win32::System::Registry::HKEY_CURRENT_USER,
                                windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
                                windows::core::w!("Rpaper"),
                            );
                        }

                        CONFIG.with(|c| {
                            let mut cfg = c.borrow_mut();
                            cfg.autostart = checked;
                            let _ = cfg.save();
                        });
                    }
                    CMD_SETTINGS_CLOSED => {
                        // 设置窗口已关闭
                        SETTINGS_HWND.with(|s| *s.borrow_mut() = None);
                    }
                    IDM_EXIT => { PostQuitMessage(0); }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => { PostQuitMessage(0); LRESULT(0) }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn create_hidden_window() -> Result<HWND, String> {
    unsafe {
        let hinst = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        let class_name = w!("WallpaperMsg");

        let wcex = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(hidden_wnd_proc),
            hInstance: hinst.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wcex);

        Ok(CreateWindowExW(
            WINDOW_EX_STYLE(0), class_name, w!("WallpaperMsg"),
            WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT,
            CW_USEDEFAULT, CW_USEDEFAULT,
            None, None, hinst, None,
        ))
    }
}

/// 加载文件并持久化到 config — 命令行启动和单实例转发共用此逻辑
/// - silent_on_error=true: 错误走 eprintln（上次文件自动加载场景）
/// - silent_on_error=false: 错误走 MessageBox（命令行 / 单实例转发场景）
fn load_file_and_persist(path: PathBuf, silent_on_error: bool) -> Result<(), String> {
    let mut ret: Result<(), String> = Ok(());
    APP.with(|a| {
        if let Some(app) = &mut *a.borrow_mut() {
            match app.load_file(path.clone()) {
                Ok(wp_loaded) => {
                    CONFIG.with(|c| {
                        let mut cfg = c.borrow_mut();
                        let path_str = path.to_string_lossy().to_string();
                        let ext = path.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.to_lowercase())
                            .unwrap_or_default();
                        match ext.as_str() {
                            "rwp" | "pkg" => cfg.last_package_path = Some(path_str),
                            "png"|"jpg"|"jpeg"|"bmp"|"webp"|"gif" => {
                                cfg.last_image_path = Some(path_str);
                            }
                            "mp4"|"mkv"|"avi"|"webm"|"mov"|"flv"|"wmv"|"ts"|"m4v" => {
                                cfg.last_video_path = Some(path_str);
                            }
                            _ => {}
                        }
                        cfg.wallpaper_type = match wp_loaded {
                            WallpaperType::Aurora => "aurora",
                            WallpaperType::Particles => "particles",
                            WallpaperType::Image => "image",
                            WallpaperType::Video => "video",
                        }.into();
                        let _ = cfg.save();
                    });
                    // 更新设置窗口的当前文件路径显示
                    SETTINGS_HWND.with(|s| {
                        if let Some(hwnd) = s.borrow().as_ref() {
                            // 只显示文件名，路径太长会溢出
                            let display = path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| path.to_string_lossy().to_string());
                            update_current_file(*hwnd, &display);
                        }
                    });
                }
                Err(e) => {
                    if silent_on_error {
                        eprintln!("[rpaper] 自动加载失败: {e}");
                    } else {
                        show_error(&e);
                    }
                    ret = Err(e);
                }
            }
        } else {
            ret = Err("App 尚未初始化".to_string());
        }
    });
    ret
}

fn main() {
    unsafe { let _ = SetProcessDPIAware(); }

    // 清理上次崩溃/异常退出残留的 .pkg 临时文件
    cleanup_stale_pkg_temp();

    // 加载持久化配置 + 解析命令行参数（先于单实例检测，便于已运行实例转发文件路径）
    let config = AppConfig::load();
    let startup = parse_args(&config);
    // 启动壁纸类型: 命令行文件先按 config 默认占位，加载成功后会更新；非文件则用关键字
    let wp = match &startup {
        StartupTarget::File(_) => default_wp_from_config(&config),
        StartupTarget::Wallpaper(w) => *w,
    };
    let cli_file = match &startup {
        StartupTarget::File(p) => Some(p.clone()),
        StartupTarget::Wallpaper(_) => None,
    };

    // 单实例检测 — 已有实例在运行时，转发命令行文件后立即退出，避免启动第二个进程
    if !acquire_single_instance() {
        if let Some(path) = &cli_file {
            let _ = forward_file_path(path);
        }
        return;
    }

    // 仅首个实例注册文件关联（避免第二个实例重复刷新 Explorer）
    if !cfg!(debug_assertions) {
        register_file_association();
    }

    CONFIG.with(|c| *c.borrow_mut() = config);

    let hidden_hwnd = match create_hidden_window() {
        Ok(h) => h,
        Err(e) => {
            show_error(&e);
            std::process::exit(1);
        }
    };
    HIDDEN_HWND.with(|h| *h.borrow_mut() = Some(hidden_hwnd));
    settings::set_hidden_hwnd(hidden_hwnd);

    // 启动视频状态刷新定时器 — 1 秒一次，函数内部会判断设置窗口是否打开
    unsafe { let _ = SetTimer(hidden_hwnd, IDM_VIDEO_STATUS_TIMER, 1000, None); }

    let tray = match TrayIcon::new(hidden_hwnd) {
        Ok(t) => t,
        Err(e) => {
            show_error(&format!("创建托盘图标失败: {e}"));
            std::process::exit(1);
        }
    };
    TRAY.with(|t| *t.borrow_mut() = Some(tray));

    let worker = match desktop::spawn_and_find_worker() {
        Ok(h) => h,
        Err(e) => {
            show_error(&format!("找不到 WorkerW 窗口: {e}"));
            std::process::exit(1);
        }
    };
    let child = match desktop::create_child_window(worker) {
        Ok(h) => h,
        Err(e) => {
            show_error(&format!("创建子窗口失败: {e}"));
            std::process::exit(1);
        }
    };

    let app = match App::new(child, wp) {
        Ok(a) => a,
        Err(e) => {
            show_error(&format!("初始化失败: {e}"));
            std::process::exit(1);
        }
    };
    APP.with(|a| *a.borrow_mut() = Some(app));

    // 接管系统壁纸: 启动时设为纯色，退出时恢复（RAII）
    // 放在所有 std::process::exit 之后，确保 Drop 必触发
    let _sys_wp_guard = sys_wallpaper::SysWallpaperGuard::take();

    // 启动加载优先级: 命令行文件 > 配置中上次文件 > 默认壁纸
    let is_cli = cli_file.is_some();
    let initial_path: Option<PathBuf> = if let Some(p) = cli_file {
        Some(p)
    } else {
        CONFIG.with(|c| {
            let cfg = c.borrow();
            match wp {
                WallpaperType::Image => cfg.last_image_path.as_ref().map(PathBuf::from),
                WallpaperType::Video => cfg.last_video_path.as_ref().map(PathBuf::from),
                _ => None,
            }
        })
    };

    if let Some(path) = initial_path {
        // 命令行文件: 不存在/解码失败都弹窗提示；上次文件: 静默失败用默认壁纸
        // 错误已在 load_file_and_persist 内部按 silent_on_error 处理，返回值忽略
        if path.exists() || is_cli {
            let _ = load_file_and_persist(path, !is_cli);
        }
    }

    let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
    loop {
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe { TranslateMessage(&msg); DispatchMessageW(&msg); }
            if msg.message == WM_QUIT {
                // 退出时保存配置
                CONFIG.with(|c| {
                    let _ = c.borrow().save();
                });
                return;
            }
        }

        APP.with(|a| {
            if let Some(app) = &mut *a.borrow_mut() {
                if let Err(e) = app.render() {
                    match e {
                        wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                            let (w, h) = desktop::get_window_size(child);
                            app.resize(w, h);
                        }
                        wgpu::SurfaceError::OutOfMemory => {
                            eprintln!("GPU 内存不足");
                        }
                        wgpu::SurfaceError::Timeout => {}
                    }
                }
            }
        });

        // 释放 APP borrow 后检查 .pkg 解压失败错误 — 避免弹窗时死锁
        // MessageBoxW 是模态的，持有 borrow 期间弹窗的消息循环会重新进入 APP.with 导致 panic
        let pending_err = APP.with(|a| {
            a.borrow_mut().as_mut().and_then(|app| app.take_pending_error())
        });
        if let Some(e) = pending_err {
            show_error(&e);
        }

        // 检查 WM_COPYDATA 转发文件加载失败的延迟弹窗
        // WM_COPYDATA 处理期间不能弹 MessageBoxW（阻塞发送方进程），错误先存这里
        let pending_load_err = PENDING_ERROR.with(|p| p.borrow_mut().take());
        if let Some(e) = pending_load_err {
            show_error(&e);
        }
    }
}
