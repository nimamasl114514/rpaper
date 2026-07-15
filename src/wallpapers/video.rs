//! 视频壁纸 — 用 ffmpeg 解码视频逐帧渲染
//!
//! ffmpeg 进程输出 raw RGBA 到 stdout，后台线程读取，
//! 主线程每帧检查是否有新帧并上传到 GPU texture

use crate::wallpaper::Wallpaper;
use bytemuck::{Pod, Zeroable};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::*;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    time: f32,
    _pad: f32,
}

type FrameSlot = Arc<Mutex<Option<Vec<u8>>>>;

pub struct VideoWallpaper {
    pipeline: RenderPipeline,
    uniform_buffer: Buffer,
    bind_group: BindGroup,
    texture: Texture,
    frame_slot: FrameSlot,
    video_width: u32,
    video_height: u32,
    _decode_thread: thread::JoinHandle<()>,
}

impl VideoWallpaper {
    pub fn load(device: &Device, queue: &Queue, format: TextureFormat, path: PathBuf) -> Result<Self, String> {
        if which::which("ffmpeg").is_err() {
            return Err("未找到 ffmpeg，请安装 ffmpeg 并添加到 PATH。\n下载: https://ffmpeg.org/download.html".into());
        }

        // 用 ffprobe 获取分辨率
        let probe = std::process::Command::new("ffprobe")
            .args(&["-v", "error", "-select_streams", "v:0",
                     "-show_entries", "stream=width,height",
                     "-of", "csv=p=0", path.to_str().unwrap()])
            .output().map_err(|e| format!("ffprobe: {e}"))?;

        let dims: Vec<u32> = String::from_utf8_lossy(&probe.stdout)
            .trim().split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if dims.len() < 2 { return Err("无法读取视频分辨率".into()); }
        let vw = dims[0];
        let vh = dims[1];

        // 创建纹理
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("video_frame"),
            size: Extent3d { width: vw, height: vh, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // 初始黑帧
        let black = vec![0u8; (vw * vh * 4) as usize];
        queue.write_texture(
            ImageCopyTexture { texture: &texture, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All },
            &black,
            ImageDataLayout { offset: 0, bytes_per_row: Some(vw * 4), rows_per_image: Some(vh) },
            Extent3d { width: vw, height: vh, depth_or_array_layers: 1 },
        );

        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("video_sampler"),
            mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear,
            address_mode_u: AddressMode::ClampToEdge, address_mode_v: AddressMode::ClampToEdge,
            ..Default::default()
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("video_shader"),
            source: ShaderSource::Wgsl(include_str!("../../shaders/image.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("video_uniforms"),
            contents: bytemuck::bytes_of(&Uniforms { resolution: [2560.0, 1600.0], time: 0.0, _pad: 0.0 }),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("video_bgl"),
            entries: &[
                BindGroupLayoutEntry { binding: 0, visibility: ShaderStages::VERTEX_FRAGMENT, ty: BindingType::Buffer { ty: BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                BindGroupLayoutEntry { binding: 1, visibility: ShaderStages::FRAGMENT, ty: BindingType::Texture { sample_type: TextureSampleType::Float { filterable: true }, view_dimension: TextureViewDimension::D2, multisampled: false }, count: None },
                BindGroupLayoutEntry { binding: 2, visibility: ShaderStages::FRAGMENT, ty: BindingType::Sampler(SamplerBindingType::Filtering), count: None },
            ],
        });

        let pl = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("video_pl"), bind_group_layouts: &[&bgl], push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("video_pipeline"), layout: Some(&pl),
            vertex: VertexState { module: &shader, entry_point: "vs_main", buffers: &[], compilation_options: Default::default() },
            fragment: Some(FragmentState { module: &shader, entry_point: "fs_main", targets: &[Some(ColorTargetState { format, blend: Some(BlendState::REPLACE), write_mask: ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: PrimitiveState::default(), depth_stencil: None, multisample: MultisampleState::default(), multiview: None, cache: None,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("video_bg"), layout: &bgl,
            entries: &[
                BindGroupEntry { binding: 0, resource: BindingResource::Buffer(BufferBinding { buffer: &uniform_buffer, offset: 0, size: None }) },
                BindGroupEntry { binding: 1, resource: BindingResource::TextureView(&texture_view) },
                BindGroupEntry { binding: 2, resource: BindingResource::Sampler(&sampler) },
            ],
        });

        // 后台解码线程
        let frame_slot: FrameSlot = Arc::new(Mutex::new(None));
        let slot_clone = frame_slot.clone();
        let path_clone = path.clone();
        let frame_size = (vw * vh * 4) as usize;

        let decode_thread = thread::spawn(move || {
            let mut child = match std::process::Command::new("ffmpeg")
                .args(&["-stream_loop", "-1",
                        "-i", path_clone.to_str().unwrap(),
                        "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn() {
                Ok(c) => c,
                Err(_) => return,
            };

            use std::io::Read;
            let stdout = child.stdout.as_mut().unwrap();
            let mut buf = vec![0u8; frame_size];
            loop {
                match stdout.read_exact(&mut buf) {
                    Ok(()) => {
                        let mut slot = slot_clone.lock().unwrap();
                        *slot = Some(buf.clone());
                    }
                    Err(_) => break,
                }
            }
            let _ = child.wait();
        });

        Ok(Self {
            pipeline, uniform_buffer, bind_group, texture,
            frame_slot, video_width: vw, video_height: vh,
            _decode_thread: decode_thread,
        })
    }

    #[allow(dead_code)]
    pub fn write_uniforms(&self, queue: &Queue, width: f32, height: f32, time: f32) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&Uniforms { resolution: [width, height], time, _pad: 0.0 }));
    }

    pub fn update_texture(&self, queue: &Queue) {
        let frame = {
            let mut slot = self.frame_slot.lock().unwrap();
            slot.take()
        };
        if let Some(data) = frame {
            queue.write_texture(
                ImageCopyTexture { texture: &self.texture, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All },
                &data,
                ImageDataLayout { offset: 0, bytes_per_row: Some(self.video_width * 4), rows_per_image: Some(self.video_height) },
                Extent3d { width: self.video_width, height: self.video_height, depth_or_array_layers: 1 },
            );
        }
    }
}

impl Wallpaper for VideoWallpaper {
    fn init(_device: &Device, _config: &SurfaceConfiguration, _format: TextureFormat) -> Self {
        unreachable!("use load()")
    }
    fn render(&self, view: &TextureView, encoder: &mut CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("video_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view, resolve_target: None,
                ops: Operations { load: LoadOp::Clear(Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }), store: StoreOp::Store },
            })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }

}
