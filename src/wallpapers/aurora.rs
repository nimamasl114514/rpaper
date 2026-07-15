//! 流动极光壁纸 — 基于 simplex noise 的多层渐变流动效果

use crate::wallpaper::Wallpaper;
use bytemuck::{Pod, Zeroable};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::*;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    resolution: [f32; 2],
    time: f32,
    _pad: f32,
}

pub struct AuroraWallpaper {
    pipeline: RenderPipeline,
    uniform_buffer: Buffer,
    bind_group: BindGroup,
}

impl Wallpaper for AuroraWallpaper {
    fn init(device: &Device, _config: &SurfaceConfiguration, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("aurora_shader"),
            source: ShaderSource::Wgsl(include_str!("../../shaders/aurora.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("aurora_uniforms"),
            contents: bytemuck::bytes_of(&Uniforms {
                resolution: [1920.0, 1080.0],
                time: 0.0,
                _pad: 0.0,
            }),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("aurora_bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("aurora_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("aurora_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("aurora_bg"),
            layout: &bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group,
        }
    }


    fn render(&self, view: &TextureView, encoder: &mut CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("aurora_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }


}

impl AuroraWallpaper {
    pub fn write_uniforms(&self, queue: &Queue, width: f32, height: f32, time: f32) {
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&Uniforms {
                resolution: [width, height],
                time,
                _pad: 0.0,
            }),
        );
    }
}
