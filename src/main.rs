//! Rpaper - 动态壁纸引擎入口
//!
//! windows subsystem 不会弹出 cmd 窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod desktop;
mod mf_decoder;
mod rwp;
mod settings;
mod tray;
mod wallpaper;
mod wallpapers;

use app::{App, WallpaperType};
use std::path::PathBuf;
use std::cell::RefCell;
use tray::*;
use settings::*;
use windows::Win32::Foundation::{HWND, HINSTANCE, LPARAM, WPARAM, LRESULT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, PeekMessageW,
    PostQuitMessage, RegisterClassExW, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    WINDOW_EX_STYLE, WM_COMMAND, WM_DESTROY, WM_QUIT,
    WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
    SetProcessDPIAware, MessageBoxW, MB_OK, MB_ICONERROR,
};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OPENFILENAMEW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::core::{w, PCWSTR};
use windows::Win32::UI::WindowsAndMessaging::PM_REMOVE;
use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

thread_local! {
    static APP: RefCell<Option<App>> = RefCell::new(None);
    static TRAY: RefCell<Option<TrayIcon>> = RefCell::new(None);
    static HIDDEN_HWND: RefCell<Option<HWND>> = RefCell::new(None);
    static SETTINGS_HWND: RefCell<Option<HWND>> = RefCell::new(None);
}

fn parse_args() -> WallpaperType {
    match std::env::args().nth(1).as_deref() {
        Some("particles" | "particle" | "p") => WallpaperType::Particles,
        Some("image" | "i") => WallpaperType::Image,
        Some("video" | "v") => WallpaperType::Video,
        _ => WallpaperType::Aurora,
    }
}

fn open_file_dialog(filter_video: bool) -> Option<PathBuf> {
    let hwnd = HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(0)));
    unsafe {
        let mut file_buf = [0u16; 260];
        let filter_str = if filter_video {
            "视频\0*.mp4;*.mkv;*.avi;*.webm;*.mov;*.flv;*.wmv;*.ts;*.m4v\0所有文件\0*.*\0"
        } else {
            "图片\0*.png;*.jpg;*.jpeg;*.bmp;*.webp;*.gif\0所有文件\0*.*\0"
        };
        let filter: Vec<u16> = filter_str.encode_utf16().collect();

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
            lpstrTitle: PCWSTR::null(),
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

fn open_file_dialog_audio() -> Option<PathBuf> {
    let hwnd = HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(0)));
    unsafe {
        let mut file_buf = [0u16; 260];
        let filter: Vec<u16> = "音频文件\0*.mp3;*.wav;*.ogg;*.flac;*.m4a;*.aac\0所有文件\0*.*\0".encode_utf16().collect();
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
            lpstrTitle: PCWSTR::null(),
            Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
            nFileOffset: 0, nFileExtension: 0, lpstrDefExt: PCWSTR::null(),
            lCustData: LPARAM(0), lpfnHook: None, lpTemplateName: PCWSTR::null(),
            pvReserved: std::ptr::null_mut(), dwReserved: 0,
            FlagsEx: windows::Win32::UI::Controls::Dialogs::OPEN_FILENAME_FLAGS_EX(0),
        };
        if GetOpenFileNameW(&mut ofn).as_bool() {
            let len = file_buf.iter().position(|&c| c == 0).unwrap_or(0);
            Some(PathBuf::from(String::from_utf16_lossy(&file_buf[..len])))
        } else { None }
    }
}

fn open_file_dialog_rwp() -> Option<PathBuf> {
    let hwnd = HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(0)));
    unsafe {
        let mut file_buf = [0u16; 260];
        let filter: Vec<u16> = "Rpaper 壁纸包\0*.rwp\0所有文件\0*.*\0".encode_utf16().collect();
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
            lpstrTitle: PCWSTR::null(),
            Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
            nFileOffset: 0, nFileExtension: 0, lpstrDefExt: PCWSTR::null(),
            lCustData: LPARAM(0), lpfnHook: None, lpTemplateName: PCWSTR::null(),
            pvReserved: std::ptr::null_mut(), dwReserved: 0,
            FlagsEx: windows::Win32::UI::Controls::Dialogs::OPEN_FILENAME_FLAGS_EX(0),
        };
        if GetOpenFileNameW(&mut ofn).as_bool() {
            let len = file_buf.iter().position(|&c| c == 0).unwrap_or(0);
            Some(PathBuf::from(String::from_utf16_lossy(&file_buf[..len])))
        } else { None }
    }
}

fn show_error(msg: &str) {
    let hwnd = HIDDEN_HWND.with(|h| h.borrow().unwrap_or(HWND(0)));
    let wstr: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "Rpaper 错误\0".encode_utf16().collect();
    unsafe {
        let _ = MessageBoxW(hwnd, PCWSTR(wstr.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR);
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
                    0x0204 /* WM_LBUTTONUP */ => {
                        APP.with(|a| {
                            if let Some(app) = &mut *a.borrow_mut() {
                                let next = match app.current_wallpaper() {
                                    WallpaperType::Aurora => WallpaperType::Particles,
                                    WallpaperType::Particles => WallpaperType::Aurora,
                                    _ => WallpaperType::Aurora,
                                };
                                app.switch_wallpaper(next);
                            }
                        });
                        LRESULT(0)
                    }
                    _ => LRESULT(0),
                }
            }
            WM_COMMAND => {
                let cmd = (wparam.0 as u32) & 0xFFFF;
                match cmd as usize {
                    IDM_AURORA => {
                        APP.with(|a| { if let Some(app) = &mut *a.borrow_mut() { app.switch_wallpaper(WallpaperType::Aurora); } });
                    }
                    IDM_PARTICLES => {
                        APP.with(|a| { if let Some(app) = &mut *a.borrow_mut() { app.switch_wallpaper(WallpaperType::Particles); } });
                    }
                    IDM_IMAGE => {
                        if let Some(path) = open_file_dialog(false) {
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    if let Err(e) = app.load_file(path) { show_error(&e); }
                                }
                            });
                        }
                    }
                    IDM_VIDEO => {
                        if let Some(path) = open_file_dialog(true) {
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    if let Err(e) = app.load_file(path) { show_error(&e); }
                                }
                            });
                        }
                    }
                    IDM_PACKAGE => {
                        if let Some(path) = open_file_dialog_rwp() {
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    if let Err(e) = app.load_file(path) { show_error(&e); }
                                }
                            });
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
                                        SETTINGS_HWND.with(|s| *s.borrow_mut() = Some(win.hwnd));
                                    }
                                    Err(e) => show_error(&e),
                                }
                            }
                        });
                    }
                    CMD_VOLUME_CHANGED => {
                        let vol = lparam.0 as f32 / 100.0;
                        APP.with(|a| { if let Some(app) = &mut *a.borrow_mut() { app.set_volume(vol); } });
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
                        if let Some(path) = open_file_dialog(false) {
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    if let Err(e) = app.load_file(path) { show_error(&e); }
                                    SETTINGS_HWND.with(|s| {
                                        if let Some(hwnd) = s.borrow().as_ref() {
                                            update_wallpaper_selection(*hwnd, app.current_wallpaper_id());
                                        }
                                    });
                                }
                            });
                        }
                    }
                    CMD_SELECT_VIDEO => {
                        if let Some(path) = open_file_dialog(true) {
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    if let Err(e) = app.load_file(path) { show_error(&e); }
                                    SETTINGS_HWND.with(|s| {
                                        if let Some(hwnd) = s.borrow().as_ref() {
                                            update_wallpaper_selection(*hwnd, app.current_wallpaper_id());
                                        }
                                    });
                                }
                            });
                        }
                    }
                    CMD_SELECT_PACKAGE => {
                        if let Some(path) = open_file_dialog_rwp() {
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    if let Err(e) = app.load_file(path) { show_error(&e); }
                                    SETTINGS_HWND.with(|s| {
                                        if let Some(hwnd) = s.borrow().as_ref() {
                                            update_wallpaper_selection(*hwnd, app.current_wallpaper_id());
                                        }
                                    });
                                }
                            });
                        }
                    }
                    CMD_OPEN_AUDIO => {
                        if let Some(path) = open_file_dialog_audio() {
                            APP.with(|a| {
                                if let Some(app) = &mut *a.borrow_mut() {
                                    match app.load_audio_file(&path) {
                                        Ok(()) => {
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
                        }
                    }
                    CMD_AUTOSTART_TOGGLE => {
                        // 开机自启: 写注册表
                        let exe = std::env::current_exe().unwrap_or_default();
                        let exe_path = exe.to_string_lossy().to_string();
                        let _ = windows::Win32::System::Registry::RegSetKeyValueW(
                            windows::Win32::System::Registry::HKEY_CURRENT_USER,
                            windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
                            windows::core::w!("Rpaper"),
                            windows::Win32::System::Registry::REG_SZ.0,
                            Some(exe_path.as_ptr() as *const _),
                            ((exe_path.len() + 1) * 2) as u32,
                        );
                    }
                    0xDEAD => {
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

fn create_hidden_window() -> HWND {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
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

        CreateWindowExW(
            WINDOW_EX_STYLE(0), class_name, w!("WallpaperMsg"),
            WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT,
            CW_USEDEFAULT, CW_USEDEFAULT,
            None, None, hinst, None,
        )
    }
}

fn main() {
    unsafe { let _ = SetProcessDPIAware(); }

    let wp = parse_args();

    let hidden_hwnd = create_hidden_window();
    HIDDEN_HWND.with(|h| *h.borrow_mut() = Some(hidden_hwnd));
    settings::set_hidden_hwnd(hidden_hwnd);

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

    let app = App::new(child, wp);
    APP.with(|a| *a.borrow_mut() = Some(app));

    let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
    loop {
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe { TranslateMessage(&msg); DispatchMessageW(&msg); }
            if msg.message == WM_QUIT { return; }
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
    }
}
