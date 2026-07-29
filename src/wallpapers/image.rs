//! 图片壁纸 — 加载本地图片文件作为壁纸

use crate::wallpaper::Wallpaper;
use bytemuck::{Pod, Zeroable};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::*;
use std::path::Path;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    time: f32,
    _pad: f32,
}

pub struct ImageWallpaper {
    pipeline: RenderPipeline,
    uniform_buffer: Buffer,
    bind_group: BindGroup,
    loaded_path: Option<String>,
}

impl ImageWallpaper {
    pub fn load(device: &Device, queue: &Queue, format: TextureFormat, path: &Path) -> std::io::Result<Self> {
        let img = image::open(path)
            .map_err(|e| std::io::Error::other(format!("image: {e}")))?;
        let rgba = img.to_rgba8();
        Ok(Self::from_rgba(device, queue, format, rgba, Some(path.to_string_lossy().into_owned())))
    }

    /// 从内存数据加载图片（用于 .rwp 壁纸包，避免写临时文件）。
    /// `name` 仅用于标记 loaded_path（使 has_image() 仍有效），
    /// 图片格式由 image::load_from_memory 从魔数自动识别，无需后缀推断。
    pub fn load_from_memory(data: &[u8], name: &str, device: &Device, queue: &Queue, format: TextureFormat) -> Result<Self, String> {
        let img = image::load_from_memory(data)
            .map_err(|e| format!("image: {e}"))?;
        let rgba = img.to_rgba8();
        Ok(Self::from_rgba(device, queue, format, rgba, Some(name.to_string())))
    }

    /// 公共构造：从 RGBA 像素缓冲创建纹理 + 完成 pipeline/bind_group 组装。
    /// `load` 与 `load_from_memory` 共用此逻辑。
    fn from_rgba(device: &Device, queue: &Queue, format: TextureFormat, rgba: image::RgbaImage, loaded: Option<String>) -> Self {
        let (width, height) = (rgba.width(), rgba.height());

        let texture = device.create_texture(&TextureDescriptor {
            label: Some("wallpaper_image"),
            size: Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            ImageCopyTexture {
                texture: &texture, mip_level: 0,
                origin: Origin3d::ZERO, aspect: TextureAspect::All,
            },
            &rgba,
            ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            Extent3d { width, height, depth_or_array_layers: 1 },
        );

        Self::finish(device, format, texture, loaded)
    }

    pub fn placeholder(device: &Device, format: TextureFormat) -> Self {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("placeholder"),
            size: Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self::finish(device, format, texture, None)
    }

    fn finish(device: &Device, format: TextureFormat, texture: Texture, path: Option<String>) -> Self {
        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("wallpaper_sampler"),
            mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear,
            address_mode_u: AddressMode::ClampToEdge, address_mode_v: AddressMode::ClampToEdge,
            ..Default::default()
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("image_shader"),
            source: ShaderSource::Wgsl(include_str!("../../shaders/image.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("image_uniforms"),
            contents: bytemuck::bytes_of(&Uniforms { resolution: [2560.0, 1600.0], time: 0.0, _pad: 0.0 }),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("image_bgl"),
            entries: &[
                BindGroupLayoutEntry { binding: 0, visibility: ShaderStages::VERTEX_FRAGMENT, ty: BindingType::Buffer { ty: BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                BindGroupLayoutEntry { binding: 1, visibility: ShaderStages::FRAGMENT, ty: BindingType::Texture { sample_type: TextureSampleType::Float { filterable: true }, view_dimension: TextureViewDimension::D2, multisampled: false }, count: None },
                BindGroupLayoutEntry { binding: 2, visibility: ShaderStages::FRAGMENT, ty: BindingType::Sampler(SamplerBindingType::Filtering), count: None },
            ],
        });

        let pl = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("image_pl"), bind_group_layouts: &[&bgl], push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("image_pipeline"), layout: Some(&pl),
            vertex: VertexState { module: &shader, entry_point: "vs_main", buffers: &[], compilation_options: Default::default() },
            fragment: Some(FragmentState { module: &shader, entry_point: "fs_main", targets: &[Some(ColorTargetState { format, blend: Some(BlendState::REPLACE), write_mask: ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: PrimitiveState::default(), depth_stencil: None, multisample: MultisampleState::default(), multiview: None, cache: None,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("image_bg"), layout: &bgl,
            entries: &[
                BindGroupEntry { binding: 0, resource: BindingResource::Buffer(BufferBinding { buffer: &uniform_buffer, offset: 0, size: None }) },
                BindGroupEntry { binding: 1, resource: BindingResource::TextureView(&texture_view) },
                BindGroupEntry { binding: 2, resource: BindingResource::Sampler(&sampler) },
            ],
        });

        Self { pipeline, uniform_buffer, bind_group, loaded_path: path }
    }

    pub fn write_uniforms(&self, queue: &Queue, width: f32, height: f32, time: f32) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&Uniforms { resolution: [width, height], time, _pad: 0.0 }));
    }

    pub fn loaded_path(&self) -> Option<&str> { self.loaded_path.as_deref() }
}

impl Wallpaper for ImageWallpaper {
    fn init(device: &Device, _config: &SurfaceConfiguration, format: TextureFormat) -> Self {
        Self::placeholder(device, format)
    }

    fn render(&self, view: &TextureView, encoder: &mut CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("image_pass"),
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
