//! 应用核心 — wgpu 初始化 + 渲染循环 + 壁纸切换

use crate::desktop;
use crate::wallpaper::Wallpaper;
use crate::wallpapers::aurora::AuroraWallpaper;
use crate::wallpapers::image::ImageWallpaper;
use crate::wallpapers::particle::ParticleWallpaper;
use crate::wallpapers::video::VideoWallpaper;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, Win32WindowHandle, WindowHandle, WindowsDisplayHandle,
};
use crate::audio::AudioPlayer;
use crate::pkg::{probe_pkg, PkgVideo};
use crate::rwp::WallpaperPackage;
use crate::video::decoder::DecoderStatus;
use std::num::NonZeroIsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use wgpu::*;
use windows::Win32::Foundation::HWND;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WallpaperType {
    Aurora,
    Particles,
    Image,
    Video,
}

/// .pkg 后台解压状态 — 给 UI 显示用
/// 与 DecoderState 解耦：解压发生在 VideoDecoder 创建之前，不应污染解码器状态
#[derive(Debug, Clone)]
pub enum PkgStatus {
    /// 正在后台线程解压 .pkg
    Extracting,
    /// 解压失败（错误信息同时由 take_pending_error 弹窗）
    Failed(String),
}

/// 后台解压任务 — 持有 JoinHandle + 共享结果槽
struct PkgExtractTask {
    handle: thread::JoinHandle<()>,
    /// None=进行中, Some(Ok(_))=成功待消费, Some(Err(_))=失败待处理
    result: Arc<Mutex<Option<Result<PkgVideo, String>>>>,
}

struct HwndWrapper { hwnd: isize }

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let h = Win32WindowHandle::new(
            NonZeroIsize::new(self.hwnd).ok_or(HandleError::Unavailable)?,
        );
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(h)) })
    }
}

impl HasDisplayHandle for HwndWrapper {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(RawDisplayHandle::Windows(WindowsDisplayHandle::new())) })
    }
}

unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}

pub struct App {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    wallpaper_type: WallpaperType,
    aurora: AuroraWallpaper,
    particles: ParticleWallpaper,
    image: ImageWallpaper,
    video: Option<VideoWallpaper>,
    audio: Option<AudioPlayer>,
    /// 当前 .pkg 解出的临时视频文件，随 App 生命周期存活，避免文件过早释放
    pkg_video: Option<PkgVideo>,
    /// .pkg 后台解压任务 — 进行中时 video=None，render 走 Aurora 占位
    pkg_extracting: Option<PkgExtractTask>,
    /// 解压失败错误信息 — 状态栏显示用，主循环 take_pending_error 弹窗后保留显示
    pkg_error: Option<String>,
    /// 避免重复弹窗的标志（同一条错误只弹一次）
    pkg_error_shown: bool,
    paused: bool,
    start_time: Instant,
    last_frame: Instant,
    width: u32,
    height: u32,
}

impl App {
    pub fn new(hwnd: HWND, wallpaper_type: WallpaperType) -> Result<Self, String> {
        let wrapper = HwndWrapper { hwnd: hwnd.0 };
        let (width, height) = desktop::get_window_size(hwnd);

        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::DX12,
            ..Default::default()
        });

        let surface = instance.create_surface(wrapper)
            .map_err(|e| format!("创建 Surface 失败: {e}"))?;

        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or("未找到兼容的 GPU 适配器（需要 DirectX 12）".to_string())?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &DeviceDescriptor {
                label: None,
                required_features: Features::empty(),
                required_limits: Limits {
                    max_texture_dimension_2d: 8192,
                    max_storage_buffers_per_shader_stage: 4,
                    max_storage_buffer_binding_size: 512 * 1024,
                    ..Limits::default()
                },
                memory_hints: MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|e| format!("请求 GPU 设备失败: {e}"))?;

        let caps = surface.get_capabilities(&adapter);
        if caps.formats.is_empty() {
            return Err("Surface 不支持任何像素格式".into());
        }
        let format = caps.formats.iter().copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let aurora = AuroraWallpaper::init(&device, &config, format);
        let particles = ParticleWallpaper::init(&device, &config, format);
        let image = ImageWallpaper::placeholder(&device, format);

        Ok(Self {
            surface, device, queue, config, wallpaper_type,
            aurora, particles, image, video: None, audio: None, pkg_video: None,
            pkg_extracting: None, pkg_error: None, pkg_error_shown: false,
            paused: false,
            start_time: Instant::now(),
            last_frame: Instant::now(),
            width: width.max(1), height: height.max(1),
        })
    }

    pub fn switch_wallpaper(&mut self, wp: WallpaperType) {
        self.wallpaper_type = wp;
    }

    /// 根据文件后缀自动选择壁纸类型并加载
    pub fn load_file(&mut self, path: PathBuf) -> Result<WallpaperType, String> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        // 壁纸包 .rwp
        if ext == "rwp" {
            return self.load_package(&path);
        }

        // Wallpaper Engine .pkg (仅 video 类型)
        if ext == "pkg" {
            return self.load_pkg_video(&path);
        }

        // 加载新壁纸前停掉旧音频 + 释放旧 .pkg 临时文件
        self.stop_audio();
        self.pkg_video = None;

        match ext.as_str() {
            // 图片格式
            "png" | "jpg" | "jpeg" | "bmp" | "webp" | "gif" => {
                let format = self.config.format;
                let new_image = ImageWallpaper::load(&self.device, &self.queue, format, &path)
                    .map_err(|e| e.to_string())?;
                self.image = new_image;
                self.wallpaper_type = WallpaperType::Image;
                Ok(WallpaperType::Image)
            }
            // 视频格式
            "mp4" | "mkv" | "avi" | "webm" | "mov" | "flv" | "wmv" | "ts" | "m4v" => {
                let video = VideoWallpaper::load(&self.device, &self.queue, self.config.format, path)
                    .map_err(|e| e.to_string())?;
                self.video = Some(video);
                self.wallpaper_type = WallpaperType::Video;
                Ok(WallpaperType::Video)
            }
            _ => Err(format!("不支持的文件格式: .{ext}\n支持: png/jpg/bmp/webp/gif/mp4/mkv/avi/webm/mov/rwp/pkg"))
        }
    }

    /// 加载 Wallpaper Engine .pkg (仅 video 类型)
    ///
    /// 两阶段加载:
    ///   1. probe 阶段同步执行（毫秒级）— 打开文件 + 解析头 + 读 project.json + 定位条目
    ///   2. extract 阶段在后台线程执行（耗时）— LZ4 解压视频到临时文件
    ///
    /// 立即返回 Video 占位，render 循环 poll 解压状态后切换实际视频。
    /// 解压期间 video=None，render 自动 fallback 到 Aurora 占位画面。
    fn load_pkg_video(&mut self, path: &Path) -> Result<WallpaperType, String> {
        // 1. probe — 同步快速打开 + 解析头 + 定位视频/音频条目（毫秒级）
        let probe = probe_pkg(path).map_err(|e| e.to_string())?;

        // 2. 取消旧的解压任务（join 等线程结束，避免临时文件冲突 + 释放旧资源）
        if let Some(task) = self.pkg_extracting.take() {
            let _ = task.handle.join();
        }
        self.pkg_video = None;     // 释放旧 .pkg 临时文件
        self.video = None;         // 释放旧 VideoWallpaper
        self.stop_audio();
        self.pkg_error = None;
        self.pkg_error_shown = false;

        // 3. 启动后台解压线程 — 共享结果槽供主线程 poll
        let result = Arc::new(Mutex::new(None));
        let result_clone = result.clone();
        let handle = thread::spawn(move || {
            let r = probe.extract().map_err(|e| e.to_string());
            if let Ok(mut guard) = result_clone.lock() {
                *guard = Some(r);
            }
        });
        self.pkg_extracting = Some(PkgExtractTask { handle, result });

        // 4. 立即返回 Video 占位 — render 时 video=None 自动 fallback 到 Aurora
        self.wallpaper_type = WallpaperType::Video;
        Ok(WallpaperType::Video)
    }

    /// poll 后台解压状态 — 由 render 循环每帧调用
    /// 完成时取出结果，成功则加载视频，失败则切回 Aurora + 设置错误
    fn poll_pkg_extract(&mut self) {
        // 先尝试取结果（不持有 borrow 跨越调用）
        let result = self
            .pkg_extracting
            .as_ref()
            .and_then(|task| task.result.lock().ok().and_then(|mut g| g.take()));

        if let Some(result) = result {
            // 任务完成，join 线程回收资源
            if let Some(task) = self.pkg_extracting.take() {
                let _ = task.handle.join();
            }
            match result {
                Ok(pkg_video) => self.finish_pkg_extract(pkg_video),
                Err(e) => {
                    self.pkg_error = Some(e);
                    self.pkg_error_shown = false;
                    // 切回 Aurora 占位（video=None 时 render 已自动 fallback）
                    self.wallpaper_type = WallpaperType::Aurora;
                }
            }
        }
    }

    /// 解压完成 — 加载音频 + 创建 VideoWallpaper
    /// 视频创建失败也走错误流程（切回 Aurora + 弹窗）
    fn finish_pkg_extract(&mut self, pkg_video: PkgVideo) {
        self.stop_audio();

        if let Some(audio_data) = pkg_video.audio_data.clone() {
            let ext = pkg_video.audio_ext.clone();
            match AudioPlayer::load_loop(audio_data, &ext) {
                Ok(player) => {
                    self.audio = Some(player);
                }
                Err(e) => {
                    eprintln!("[rpaper] .pkg 音频加载失败: {e}");
                }
            }
        }

        match VideoWallpaper::load(
            &self.device,
            &self.queue,
            self.config.format,
            pkg_video.video_temp_path.clone(),
        ) {
            Ok(video) => {
                self.video = Some(video);
                self.pkg_video = Some(pkg_video);
                self.wallpaper_type = WallpaperType::Video;
            }
            Err(e) => {
                self.pkg_error = Some(e);
                self.pkg_error_shown = false;
                self.wallpaper_type = WallpaperType::Aurora;
            }
        }
    }

    /// 对外暴露 .pkg 解压状态 — 给设置窗口状态栏显示用
    pub fn pkg_status(&self) -> Option<PkgStatus> {
        if self.pkg_extracting.is_some() {
            return Some(PkgStatus::Extracting);
        }
        if let Some(e) = &self.pkg_error {
            return Some(PkgStatus::Failed(e.clone()));
        }
        None
    }

    /// 主循环每帧调用 — 返回需要弹窗的错误（首次返回，后续 None）
    /// 在 render 之后调用，避免弹窗阻塞时持有 APP borrow 死锁
    pub fn take_pending_error(&mut self) -> Option<String> {
        if self.pkg_error.is_some() && !self.pkg_error_shown {
            self.pkg_error_shown = true;
            self.pkg_error.clone()
        } else {
            None
        }
    }

    pub fn current_wallpaper(&self) -> WallpaperType { self.wallpaper_type }
    pub fn current_wallpaper_id(&self) -> u32 {
        match self.wallpaper_type {
            WallpaperType::Aurora => 0,
            WallpaperType::Particles => 1,
            WallpaperType::Image => 2,
            WallpaperType::Video => 3,
        }
    }
    pub fn has_image(&self) -> bool { self.image.loaded_path().is_some() }
    pub fn has_video(&self) -> bool { self.video.is_some() }

    /// 视频解码器状态快照（仅视频壁纸有意义，其他类型返回 None）
    pub fn video_status(&self) -> Option<DecoderStatus> {
        self.video.as_ref().and_then(|v| v.status())
    }

    /// 视频宽度（仅视频壁纸有意义，其他返回 0）
    pub fn video_width(&self) -> u32 {
        self.video.as_ref().map(|v| v.video_width()).unwrap_or(0)
    }

    /// 视频高度（仅视频壁纸有意义，其他返回 0）
    pub fn video_height(&self) -> u32 {
        self.video.as_ref().map(|v| v.video_height()).unwrap_or(0)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        self.width = width;
        self.height = height;
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self) -> Result<(), SurfaceError> {
        // 先 poll .pkg 后台解压状态 — 完成则切换视频/记录错误
        self.poll_pkg_extract();

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        if self.paused {
            // 暂停时仍然渲染当前帧（保持画面），但不更新时间
            let frame = self.surface.get_current_texture()?;
            let view = frame.texture.create_view(&TextureViewDescriptor::default());
            let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor { label: None });
            match self.wallpaper_type {
                WallpaperType::Aurora => { self.aurora.render(&view, &mut encoder); }
                WallpaperType::Particles => { self.particles.render(&view, &mut encoder); }
                WallpaperType::Image => {
                    if self.image.loaded_path().is_some() {
                        self.image.render(&view, &mut encoder);
                    } else {
                        self.aurora.render(&view, &mut encoder);
                    }
                }
                WallpaperType::Video => {
                    if let Some(v) = &self.video {
                        v.render(&view, &mut encoder);
                    } else {
                        self.aurora.render(&view, &mut encoder);
                    }
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            frame.present();
            return Ok(());
        }
        
        let elapsed = now.duration_since(self.start_time).as_secs_f32();

        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor { label: None });

        match self.wallpaper_type {
            WallpaperType::Aurora => {
                self.aurora.write_uniforms(&self.queue, self.width as f32, self.height as f32, elapsed);
                self.aurora.render(&view, &mut encoder);
            }
            WallpaperType::Particles => {
                self.particles.write_uniforms(&self.queue, self.width as f32, self.height as f32, elapsed, dt);
                self.particles.render(&view, &mut encoder);
            }
            WallpaperType::Image => {
                if self.image.loaded_path().is_some() {
                    self.image.write_uniforms(&self.queue, self.width as f32, self.height as f32, elapsed);
                    self.image.render(&view, &mut encoder);
                } else {
                    self.aurora.write_uniforms(&self.queue, self.width as f32, self.height as f32, elapsed);
                    self.aurora.render(&view, &mut encoder);
                }
            }
            WallpaperType::Video => {
                if let Some(video) = &self.video {
                    video.update_texture(&self.queue);
                    video.write_uniforms(&self.queue, self.width as f32, self.height as f32, elapsed);
                    video.render(&view, &mut encoder);
                } else {
                    self.aurora.write_uniforms(&self.queue, self.width as f32, self.height as f32, elapsed);
                    self.aurora.render(&view, &mut encoder);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// 加载 .rwp 壁纸包
    fn load_package(&mut self, path: &Path) -> Result<WallpaperType, String> {
        let mut pkg = WallpaperPackage::load(path).map_err(|e| format!("壁纸包加载失败: {e}"))?;
        self.stop_audio();

        // 播放音频
        if let Some(audio_data) = pkg.audio_data.take() {
            let hint = pkg.audio_name.as_deref().unwrap_or("mp3");
            let ext = hint.rsplit('.').next().unwrap_or("mp3");
            match AudioPlayer::load_loop(audio_data, ext) {
                Ok(player) => { self.audio = Some(player); }
                Err(e) => { eprintln!("[rpaper] 音频加载失败: {e}"); }
            }
        }

        match pkg.manifest.wallpaper_type.as_str() {
            "shader" | "aurora" => {
                // shader 类型壁纸包 — 目前用内置 Aurora（自定义 shader 后续支持）
                self.wallpaper_type = WallpaperType::Aurora;
                Ok(WallpaperType::Aurora)
            }
            "particles" | "particle" => {
                self.wallpaper_type = WallpaperType::Particles;
                Ok(WallpaperType::Particles)
            }
            "image" => {
                if let Some(data) = pkg.image_data.take() {
                    let img_name = pkg.image_name.take().unwrap_or_else(|| "image.png".into());
                    let format = self.config.format;
                    // 直接从内存解码，不再写临时文件（避免临时图片文件泄漏）
                    let new_image = ImageWallpaper::load_from_memory(&data, &img_name, &self.device, &self.queue, format)
                        .map_err(|e| e.to_string())?;
                    self.image = new_image;
                    self.wallpaper_type = WallpaperType::Image;
                    Ok(WallpaperType::Image)
                } else {
                    Err("图片壁纸包缺少图片文件".into())
                }
            }
            "video" => {
                if let Some(vpath) = pkg.video_path.take() {
                    let video = VideoWallpaper::load(&self.device, &self.queue, self.config.format, vpath)?;
                    self.video = Some(video);
                    self.wallpaper_type = WallpaperType::Video;
                    Ok(WallpaperType::Video)
                } else {
                    Err("视频壁纸包缺少视频文件".into())
                }
            }
            other => Err(format!("未知壁纸类型: {other}\n支持: shader/particles/image/video")),
        }
    }

    pub fn set_volume(&mut self, vol: f32) {
        let vol = vol.clamp(0.0, 1.0);
        if let Some(audio) = &self.audio {
            audio.set_volume(vol);
        }
    }

    pub fn toggle_pause(&mut self) -> bool {
        self.paused = !self.paused;
        if self.paused {
            if let Some(audio) = &self.audio { audio.pause(); }
        } else {
            if let Some(audio) = &self.audio { audio.resume(); }
        }
        self.paused
    }

    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool { self.paused }

    pub fn load_audio_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        let data = std::fs::read(path).map_err(|e| format!("读取音频文件失败: {e}"))?;
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_else(|| "mp3".into());
        self.stop_audio();
        let player = AudioPlayer::load_loop(data, &ext)?;
        self.audio = Some(player);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn has_audio(&self) -> bool { self.audio.is_some() }

    fn stop_audio(&mut self) {
        if let Some(audio) = self.audio.take() {
            audio.stop();
        }
    }
}
