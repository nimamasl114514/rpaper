//! WorkerW 窗口管理

use std::io;
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use windows::core::BOOL;
use windows::Win32::Foundation::{FALSE, HWND, HINSTANCE, LPARAM, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{HBRUSH, UpdateWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, EnumChildWindows, FindWindowW, GetClassNameW, GetClientRect,
    GetSystemMetrics, GetWindow, GW_HWNDNEXT, RegisterClassExW, SendMessageW, SetWindowPos,
    ShowWindow, CS_HREDRAW, CS_VREDRAW, HCURSOR, HICON, HMENU, HWND_TOP,
    SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER,
    WINDOW_EX_STYLE, WM_PAINT, WM_ERASEBKGND, WM_DESTROY, WNDCLASSEXW,
    WS_CHILD, WS_VISIBLE, PostQuitMessage,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::core::{w, PCWSTR};

const WM_SPAWN_WORKER: u32 = 0x052C;
// 用 AtomicI64 存储 HWND 指针，避免 static mut
static WORKER_HWND: AtomicI64 = AtomicI64::new(0);

fn hwnd_to_i64(hwnd: HWND) -> i64 {
    hwnd.0 as i64
}

fn i64_to_hwnd(val: i64) -> HWND {
    HWND(val as *mut _)
}

fn is_null_hwnd(hwnd: HWND) -> bool {
    hwnd.0.is_null()
}

fn find_progman() -> Option<HWND> {
    unsafe {
        let hwnd = FindWindowW(w!("Progman"), None).ok()?;
        if is_null_hwnd(hwnd) { None } else { Some(hwnd) }
    }
}

pub fn spawn_and_find_worker() -> io::Result<HWND> {
    unsafe {
        let progman = find_progman()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Progman not found"))?;

        WORKER_HWND.store(0, Ordering::SeqCst);
        let _ = SendMessageW(progman, WM_SPAWN_WORKER, Some(WPARAM(0)), Some(LPARAM(0)));
        let _ = EnumChildWindows(Some(progman), Some(find_defview_callback), LPARAM(0));

        if WORKER_HWND.load(Ordering::SeqCst) == 0 {
            let _ = SendMessageW(progman, WM_SPAWN_WORKER, Some(WPARAM(0)), Some(LPARAM(0)));
            let _ = EnumChildWindows(Some(progman), Some(find_defview_callback), LPARAM(0));
        }

        let val = WORKER_HWND.load(Ordering::SeqCst);
        if val == 0 {
            Err(io::Error::new(io::ErrorKind::NotFound, "WorkerW not found"))
        } else {
            Ok(i64_to_hwnd(val))
        }
    }
}

extern "system" fn find_defview_callback(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    unsafe {
        let mut buf = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut buf);
        let name = String::from_utf16_lossy(&buf[..len as usize]);
        if name.contains("SHELLDLL_DefView") {
            let worker = GetWindow(hwnd, GW_HWNDNEXT);
            if let Ok(w) = worker {
                if !is_null_hwnd(w) {
                    WORKER_HWND.store(hwnd_to_i64(w), Ordering::SeqCst);
                    return FALSE;
                }
            }
        }
        TRUE
    }
}

pub fn create_child_window(worker: HWND) -> io::Result<HWND> {
    unsafe {
        let hmodule = GetModuleHandleW(None)
            .map_err(|e| std::io::Error::other(format!("{e}")))?;
        let hinst = HINSTANCE(hmodule.0);
        let class_name = w!("WallpaperChild");

        let wcex = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(child_wnd_proc),
            hInstance: hinst,
            lpszClassName: class_name,
            hCursor: HCURSOR(ptr::null_mut()),
            hbrBackground: HBRUSH(ptr::null_mut()),
            hIcon: HICON(ptr::null_mut()),
            hIconSm: HICON(ptr::null_mut()),
            cbClsExtra: 0,
            cbWndExtra: 0,
            lpszMenuName: PCWSTR::null(),
        };
        let _ = RegisterClassExW(&wcex);

        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0), class_name, w!("DW"), WS_CHILD | WS_VISIBLE,
            0, 0, sw, sh, Some(worker), Some(HMENU(std::ptr::null_mut())), Some(hinst), None,
        ).map_err(|_| std::io::Error::other("CreateWindowExW failed"))?;

        if is_null_hwnd(hwnd) {
            return Err(std::io::Error::other("CreateWindowExW returned null"));
        }

        let _ = SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, sw, sh, SWP_NOACTIVATE | SWP_NOZORDER);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);
        Ok(hwnd)
    }
}

extern "system" fn child_wnd_proc(
    hwnd: HWND, msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => windows::Win32::Foundation::LRESULT(1),
            WM_PAINT => {
                let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
                let _ = windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut ps);
                let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, &ps);
                windows::Win32::Foundation::LRESULT(0)
            }
            WM_DESTROY => { PostQuitMessage(0); windows::Win32::Foundation::LRESULT(0) }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub fn get_window_size(hwnd: HWND) -> (u32, u32) {
    unsafe {
        let mut r = RECT::default();
        let _ = GetClientRect(hwnd, &mut r);
        ((r.right - r.left).max(1) as u32, (r.bottom - r.top).max(1) as u32)
    }
}
