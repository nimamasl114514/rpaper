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
use crate::rwp::WallpaperPackage;
use std::num::NonZeroIsize;
use std::path::PathBuf;
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

struct HwndWrapper { hwnd: isize }

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let h = Win32WindowHandle::new(NonZeroIsize::new(self.hwnd).unwrap());
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
    paused: bool,
    start_time: Instant,
    last_frame: Instant,
    width: u32,
    height: u32,
}

impl App {
    pub fn new(hwnd: HWND, wallpaper_type: WallpaperType) -> Self {
        let wrapper = HwndWrapper { hwnd: hwnd.0 as isize };
        let (width, height) = desktop::get_window_size(hwnd);

        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::DX12,
            ..Default::default()
        });

        let surface = instance.create_surface(wrapper).expect("surface");

        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("adapter");

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
        .expect("device");

        let caps = surface.get_capabilities(&adapter);
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

        Self {
            surface, device, queue, config, wallpaper_type,
            aurora, particles, image, video: None, audio: None, paused: false,
            start_time: Instant::now(),
            last_frame: Instant::now(),
            width: width.max(1), height: height.max(1),
        }
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

        // 加载新壁纸前停掉旧音频
        self.stop_audio();

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
                let video = VideoWallpaper::load(&self.device, &self.queue, self.config.format, path)?;
                self.video = Some(video);
                self.wallpaper_type = WallpaperType::Video;
                Ok(WallpaperType::Video)
            }
            _ => Err(format!("不支持的文件格式: .{ext}\n支持: png/jpg/bmp/webp/gif/mp4/mkv/avi/webm/mov/rwp"))
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

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        self.width = width;
        self.height = height;
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self) -> Result<(), SurfaceError> {
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
                WallpaperType::Image => { self.image.render(&view, &mut encoder); }
                WallpaperType::Video => { if let Some(v) = &self.video { v.render(&view, &mut encoder); } }
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
                self.image.write_uniforms(&self.queue, self.width as f32, self.height as f32, elapsed);
                self.image.render(&view, &mut encoder);
            }
            WallpaperType::Video => {
                if let Some(video) = &self.video {
                    video.update_texture(&self.queue);
                    video.write_uniforms(&self.queue, self.width as f32, self.height as f32, elapsed);
                    video.render(&view, &mut encoder);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// 加载 .rwp 壁纸包
    fn load_package(&mut self, path: &PathBuf) -> Result<WallpaperType, String> {
        let pkg = WallpaperPackage::load(path).map_err(|e| format!("壁纸包加载失败: {e}"))?;
        self.stop_audio();

        // 播放音频
        if let Some(audio_data) = pkg.audio_data {
            let hint = pkg.audio_name.as_deref().unwrap_or("mp3");
            let ext = hint.rsplit('.').next().unwrap_or("mp3");
            match AudioPlayer::load_loop(audio_data, ext) {
                Ok(player) => { self.audio = Some(player); }
                Err(e) => { eprintln!("音频: {e}"); }
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
                if let Some(data) = pkg.image_data {
                    let img_name = pkg.image_name.unwrap_or_else(|| "image.png".into());
                    let tmp = std::env::temp_dir().join(format!("rpaper_img_{}", img_name));
                    std::fs::write(&tmp, &data).map_err(|e| e.to_string())?;
                    let format = self.config.format;
                    let new_image = ImageWallpaper::load(&self.device, &self.queue, format, &tmp)
                        .map_err(|e| e.to_string())?;
                    self.image = new_image;
                    self.wallpaper_type = WallpaperType::Image;
                    Ok(WallpaperType::Image)
                } else {
                    Err("图片壁纸包缺少图片文件".into())
                }
            }
            "video" => {
                if let Some(vpath) = pkg.video_path {
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

    pub fn has_audio(&self) -> bool { self.audio.is_some() }

    fn stop_audio(&mut self) {
        if let Some(audio) = self.audio.take() {
            audio.stop();
        }
    }
}
