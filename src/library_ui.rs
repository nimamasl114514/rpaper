//! Slint UI 模块 — 现代化壁纸库窗口
//!
//! 独立线程运行 Slint 事件循环，通过 channel 与主线程通信。
//! 壁纸渲染在 WorkerW 子窗口，UI 是独立顶层窗口，互不干扰。

use slint::ComponentHandle;
use std::sync::mpsc;
use std::thread;

slint::include_modules!();

/// UI 发送给主线程的命令
#[derive(Debug, Clone)]
pub enum UiCommand {
    /// 关闭库窗口（用户点 X）
    Close,
    /// 最小化窗口
    Minimize,
    /// 添加壁纸（打开文件选择）
    AddWallpaper,
    /// 选中壁纸（卡片点击）
    SelectWallpaper(i32),
    /// 应用壁纸（双击或点应用按钮）
    ApplyWallpaper(i32),
    /// 调整音量 (0.0~1.0)
    SetVolume(f32),
    /// 搜索
    Search(String),
    /// 打开旧版设置窗口
    OpenSettings,
}

/// 主线程发送给 UI 的事件
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// 显示窗口
    Show,
    /// 隐藏窗口
    Hide,
    /// 退出事件循环（线程关闭）
    Quit,
    /// 更新当前播放的壁纸 ID
    SetPlaying(i32),
}

/// UI 控制器 — 主线程持有
pub struct LibraryUi {
    event_tx: mpsc::Sender<UiEvent>,
    cmd_rx: mpsc::Receiver<UiCommand>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl LibraryUi {
    /// 创建并启动 UI 线程（不立即显示窗口）
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<UiCommand>();
        let (event_tx, event_rx) = mpsc::channel::<UiEvent>();

        let handle = thread::spawn(move || {
            if let Err(e) = run_ui_thread(cmd_tx, event_rx) {
                eprintln!("[LibraryUI] UI 线程错误: {e}");
            }
        });

        Self {
            event_tx,
            cmd_rx,
            thread_handle: Some(handle),
        }
    }

    /// 显示窗口
    pub fn show(&self) {
        let _ = self.event_tx.send(UiEvent::Show);
    }

    /// 非阻塞尝试接收一条 UI 命令（在主循环中调用）
    pub fn try_recv(&self) -> Result<UiCommand, mpsc::TryRecvError> {
        self.cmd_rx.try_recv()
    }
}

impl Drop for LibraryUi {
    fn drop(&mut self) {
        // 通知 UI 线程退出
        let _ = self.event_tx.send(UiEvent::Quit);
        if let Some(h) = self.thread_handle.take() {
            let _ = h.join();
        }
    }
}

/// 在 UI 线程中运行 Slint
fn run_ui_thread(
    cmd_tx: mpsc::Sender<UiCommand>,
    event_rx: mpsc::Receiver<UiEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ui = LibraryWindow::new()?;
    let logic = ui.global::<LibraryLogic>();

    // === 连接所有 UI 回调 ===
    {
        let tx = cmd_tx.clone();
        logic.on_close_clicked(move || { let _ = tx.send(UiCommand::Close); });
    }
    {
        let tx = cmd_tx.clone();
        logic.on_minimize_clicked(move || { let _ = tx.send(UiCommand::Minimize); });
    }
    {
        let tx = cmd_tx.clone();
        logic.on_add_wallpaper_clicked(move || { let _ = tx.send(UiCommand::AddWallpaper); });
    }
    {
        let tx = cmd_tx.clone();
        logic.on_open_settings(move || { let _ = tx.send(UiCommand::OpenSettings); });
    }
    {
        let tx = cmd_tx.clone();
        logic.on_wallpaper_selected(move |id| { let _ = tx.send(UiCommand::SelectWallpaper(id)); });
    }
    {
        let tx = cmd_tx.clone();
        logic.on_apply_clicked(move |id| { let _ = tx.send(UiCommand::ApplyWallpaper(id)); });
    }
    {
        let tx = cmd_tx.clone();
        logic.on_volume_changed(move |v| { let _ = tx.send(UiCommand::SetVolume(v)); });
    }
    {
        let tx = cmd_tx.clone();
        logic.on_search_changed(move |v| { let _ = tx.send(UiCommand::Search(v.to_string())); });
    }

    // === 用 Timer 轮询来自主线程的事件 ===
    use slint::{Timer, TimerMode};
    let ui_weak = ui.as_weak();
    let _timer = {
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, core::time::Duration::from_millis(50), move || {
            while let Ok(event) = event_rx.try_recv() {
                let ui = match ui_weak.upgrade() {
                    Some(ui) => ui,
                    None => break,
                };
                match event {
                    UiEvent::Show => {
                        let _ = ui.show();
                        // DWM Mica + 圆角 + 深色标题栏
                        apply_dwm_effects(&ui);
                    }
                    UiEvent::Hide => {
                        let _ = ui.hide();
                    }
                    UiEvent::Quit => {
                        slint::quit_event_loop().ok();
                        break;
                    }
                    UiEvent::SetPlaying(_id) => {
                        // TODO: 更新播放状态指示
                    }
                }
            }
        });
        timer
    };

    // 运行 Slint 事件循环（阻塞直到 quit_event_loop 被调用）
    slint::run_event_loop()?;
    ui.hide()?;

    Ok(())
}

/// 给 Slint 窗口应用 DWM 效果：Mica 背景 + 圆角 + 深色标题栏
fn apply_dwm_effects(ui: &LibraryWindow) {
    use windows::core::BOOL;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW,
    };
    use slint::ComponentHandle;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    use windows::core::w;

    // Slint 窗口已 show，通过标题查找 HWND
    let hwnd = match unsafe { FindWindowW(w!("Rpaper 壁纸库"), None) } {
        Ok(h) if !h.0.is_null() => h,
        _ => return,
    };

    unsafe {
        // 圆角
        let _ = DwmSetWindowAttribute(
            hwnd, DWMWA_WINDOW_CORNER_PREFERENCE,
            &DWMWCP_ROUND as *const _ as *const _,
            std::mem::size_of_val(&DWMWCP_ROUND) as u32,
        );
        // 深色标题栏
        let dark: BOOL = BOOL(1);
        let _ = DwmSetWindowAttribute(
            hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as *const _,
            std::mem::size_of::<BOOL>() as u32,
        );
        // Mica 背景
        let backdrop = DWMSBT_MAINWINDOW;
        let _ = DwmSetWindowAttribute(
            hwnd, DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const _,
            std::mem::size_of_val(&backdrop) as u32,
        );
    }
}

