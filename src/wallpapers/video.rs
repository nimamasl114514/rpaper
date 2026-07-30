//! 视频壁纸 — openh264 解码 + GPU YUV→RGB 转换
//!
//! 大封小切割策略：
//! - decoder 产出打包 YUV（I420 planar, 无 stride）
//! - 本模块按条带上传 3 个 R8 纹理 (Y/U/V)，GPU shader 做色彩转换
//! - 零 CPU 色彩转换，内存占用降低 75%

use crate::wallpaper::Wallpaper;
use crate::video::decoder::{VideoDecoder, FrameSlot, DecoderStatus, DecoderState};
use bytemuck::{Pod, Zeroable};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::*;
use std::path::PathBuf;

/// 条带高度 — 每次上传 64 行 Y（32 行 UV），降低单次 write_texture 阻塞
const STRIP_ROWS: u32 = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    time: f32,
    _pad: f32,
}

pub struct VideoWallpaper {
    pipeline: RenderPipeline,
    uniform_buffer: Buffer,
    bind_group: BindGroup,
    tex_y: Texture,
    tex_u: Texture,
    tex_v: Texture,
    frame_slot: FrameSlot,
    video_width: u32,
    video_height: u32,
    video_decoder: Option<VideoDecoder>,
}

impl VideoWallpaper {
    pub fn load(device: &Device, queue: &Queue, format: TextureFormat, path: PathBuf) -> Result<Self, String> {
        let decoder = VideoDecoder::open(&path)?;

        let vw = decoder.width;
        let vh = decoder.height;
        let frame_slot = decoder.frame_slot.clone();

        // 小切割：3 个 R8Unorm 纹理，分别存 Y/U/V 平面
        // Y: full resolution, U/V: half resolution (4:2:0)
        let tex_y = create_r8_texture(device, vw, vh, "video_y");
        let tex_u = create_r8_texture(device, vw / 2, vh / 2, "video_u");
        let tex_v = create_r8_texture(device, vw / 2, vh / 2, "video_v");

        // 不分配黑帧 — 用 GPU clear 代替，省 8MB 临时内存
        {
            let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("video_init_clear"),
            });
            for tex in [&tex_y, &tex_u, &tex_v] {
                encoder.clear_texture(
                    tex,
                    &ImageSubresourceRange { aspect: TextureAspect::All, base_mip_level: 0, mip_level_count: None, base_array_layer: 0, array_layer_count: None },
                );
            }
            queue.submit(std::iter::once(encoder.finish()));
        }

        let tex_y_view = tex_y.create_view(&TextureViewDescriptor::default());
        let tex_u_view = tex_u.create_view(&TextureViewDescriptor::default());
        let tex_v_view = tex_v.create_view(&TextureViewDescriptor::default());

        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("video_sampler"),
            mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear,
            address_mode_u: AddressMode::ClampToEdge, address_mode_v: AddressMode::ClampToEdge,
            ..Default::default()
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("video_shader"),
            source: ShaderSource::Wgsl(include_str!("../../shaders/video.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("video_uniforms"),
            contents: bytemuck::bytes_of(&Uniforms { resolution: [2560.0, 1600.0], time: 0.0, _pad: 0.0 }),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        // 5 个 binding: uniform + Y + U + V + sampler
        let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("video_bgl"),
            entries: &[
                BindGroupLayoutEntry { binding: 0, visibility: ShaderStages::VERTEX_FRAGMENT, ty: BindingType::Buffer { ty: BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                BindGroupLayoutEntry { binding: 1, visibility: ShaderStages::FRAGMENT, ty: BindingType::Texture { sample_type: TextureSampleType::Float { filterable: true }, view_dimension: TextureViewDimension::D2, multisampled: false }, count: None },
                BindGroupLayoutEntry { binding: 2, visibility: ShaderStages::FRAGMENT, ty: BindingType::Texture { sample_type: TextureSampleType::Float { filterable: true }, view_dimension: TextureViewDimension::D2, multisampled: false }, count: None },
                BindGroupLayoutEntry { binding: 3, visibility: ShaderStages::FRAGMENT, ty: BindingType::Texture { sample_type: TextureSampleType::Float { filterable: true }, view_dimension: TextureViewDimension::D2, multisampled: false }, count: None },
                BindGroupLayoutEntry { binding: 4, visibility: ShaderStages::FRAGMENT, ty: BindingType::Sampler(SamplerBindingType::Filtering), count: None },
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
                BindGroupEntry { binding: 1, resource: BindingResource::TextureView(&tex_y_view) },
                BindGroupEntry { binding: 2, resource: BindingResource::TextureView(&tex_u_view) },
                BindGroupEntry { binding: 3, resource: BindingResource::TextureView(&tex_v_view) },
                BindGroupEntry { binding: 4, resource: BindingResource::Sampler(&sampler) },
            ],
        });

        Ok(Self {
            pipeline, uniform_buffer, bind_group,
            tex_y, tex_u, tex_v,
            frame_slot,
            video_width: vw, video_height: vh,
            video_decoder: Some(decoder),
        })
    }

    #[allow(dead_code)]
    pub fn write_uniforms(&self, queue: &Queue, width: f32, height: f32, time: f32) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&Uniforms { resolution: [width, height], time, _pad: 0.0 }));
    }

    /// 小切割：逐条带上传 YUV 数据到 3 个 GPU 纹理
    /// 每条带 STRIP_ROWS 行 Y（对应 STRIP_ROWS/2 行 UV），
    /// 降低单次 write_texture 的 PCIe 阻塞
    pub fn update_texture(&self, queue: &Queue) {
        if let Some(data) = self.frame_slot.take() {
            let vw = self.video_width as usize;
            let vh = self.video_height as usize;
            let uv_w = vw / 2;
            let uv_h = vh / 2;
            let y_len = vw * vh;
            let uv_len = uv_w * uv_h;

            // Y 平面条带上传
            let strip = STRIP_ROWS as usize;
            let mut y = 0usize;
            while y < vh {
                let rows = strip.min(vh - y);
                let offset = y * vw;
                queue.write_texture(
                    ImageCopyTexture { texture: &self.tex_y, mip_level: 0, origin: Origin3d { x: 0, y: y as u32, z: 0 }, aspect: TextureAspect::All },
                    &data[offset..offset + rows * vw],
                    ImageDataLayout { offset: 0, bytes_per_row: Some(vw as u32), rows_per_image: Some(rows as u32) },
                    Extent3d { width: vw as u32, height: rows as u32, depth_or_array_layers: 1 },
                );
                y += rows;
            }

            // U 平面条带上传（半分辨率）
            let uv_strip = strip / 2;
            let mut uy = 0usize;
            while uy < uv_h {
                let rows = uv_strip.min(uv_h - uy);
                let offset = y_len + uy * uv_w;
                queue.write_texture(
                    ImageCopyTexture { texture: &self.tex_u, mip_level: 0, origin: Origin3d { x: 0, y: uy as u32, z: 0 }, aspect: TextureAspect::All },
                    &data[offset..offset + rows * uv_w],
                    ImageDataLayout { offset: 0, bytes_per_row: Some(uv_w as u32), rows_per_image: Some(rows as u32) },
                    Extent3d { width: uv_w as u32, height: rows as u32, depth_or_array_layers: 1 },
                );
                uy += rows;
            }

            // V 平面条带上传
            let mut vy = 0usize;
            while vy < uv_h {
                let rows = uv_strip.min(uv_h - vy);
                let offset = y_len + uv_len + vy * uv_w;
                queue.write_texture(
                    ImageCopyTexture { texture: &self.tex_v, mip_level: 0, origin: Origin3d { x: 0, y: vy as u32, z: 0 }, aspect: TextureAspect::All },
                    &data[offset..offset + rows * uv_w],
                    ImageDataLayout { offset: 0, bytes_per_row: Some(uv_w as u32), rows_per_image: Some(rows as u32) },
                    Extent3d { width: uv_w as u32, height: rows as u32, depth_or_array_layers: 1 },
                );
                vy += rows;
            }

            self.frame_slot.return_buf(data);
        }
    }

    pub fn status(&self) -> Option<DecoderStatus> {
        self.video_decoder.as_ref().map(|d| d.status())
    }

    #[allow(dead_code)]
    pub fn is_loading(&self) -> bool {
        self.video_decoder.as_ref()
            .map(|d| d.status().state == DecoderState::Loading)
            .unwrap_or(false)
    }

    pub fn video_width(&self) -> u32 { self.video_width }
    pub fn video_height(&self) -> u32 { self.video_height }
}

/// 创建 R8Unorm 纹理 — 存储单个 Y/U/V 平面
fn create_r8_texture(device: &Device, w: u32, h: u32, label: &str) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: TextureDimension::D2,
        format: TextureFormat::R8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    })
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
