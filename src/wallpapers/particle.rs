//! 粒子系统壁纸 — GPU 端更新粒子位置，零 CPU→GPU 传输

use crate::wallpaper::Wallpaper;
use bytemuck::{Pod, Zeroable};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::*;

const PARTICLE_COUNT: u32 = 200;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlobalUniforms {
    resolution: [f32; 2],
    time: f32,
    dt: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ParticleData {
    pos: [f32; 2],
    vel: [f32; 2],
    size: f32,
    hue: f32,
}

pub struct ParticleWallpaper {
    render_pipeline: RenderPipeline,
    compute_pipeline: ComputePipeline,
    global_uniform: Buffer,
    bind_group_render: BindGroup,
    bind_group_compute: BindGroup,
    particle_buffer: Buffer,
}

impl Wallpaper for ParticleWallpaper {
    fn init(device: &Device, _config: &SurfaceConfiguration, format: TextureFormat) -> Self {
        let render_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("particle_render_shader"),
            source: ShaderSource::Wgsl(include_str!("../../shaders/particle.wgsl").into()),
        });

        let compute_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("particle_compute_shader"),
            source: ShaderSource::Wgsl(include_str!("../../shaders/particle_compute.wgsl").into()),
        });

        let global_uniform = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("particle_global"),
            contents: bytemuck::bytes_of(&GlobalUniforms {
                resolution: [2560.0, 1600.0],
                time: 0.0,
                dt: 0.016,
            }),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        // 初始化粒子
        let mut rng_state: u64 = 123456789;
        let mut rng = || {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            (rng_state as f32) / (u64::MAX as f32)
        };

        let particles: Vec<ParticleData> = (0..PARTICLE_COUNT)
            .map(|_| ParticleData {
                pos: [rng() * 2560.0, rng() * 1600.0],
                vel: [(rng() - 0.5) * 40.0, (rng() - 0.5) * 40.0],
                size: 3.0 + rng() * 6.0,
                hue: rng(),
            })
            .collect();

        let particle_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("particles"),
            contents: bytemuck::cast_slice(&particles),
            usage: BufferUsages::VERTEX | BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });

        // Render bind group: uniform + particle buffer (vertex)
        let render_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("particle_render_bgl"),
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

        // Compute bind group: uniform + particle buffer (storage)
        let compute_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("particle_compute_bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let render_pl = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("particle_render_pl"),
            bind_group_layouts: &[&render_bgl],
            push_constant_ranges: &[],
        });

        let compute_pl = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("particle_compute_pl"),
            bind_group_layouts: &[&compute_bgl],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("particle_render_pipeline"),
            layout: Some(&render_pl),
            vertex: VertexState {
                module: &render_shader,
                entry_point: "vs_main",
                buffers: &[VertexBufferLayout {
                    array_stride: std::mem::size_of::<ParticleData>() as BufferAddress,
                    step_mode: VertexStepMode::Instance,
                    attributes: &[
                        VertexAttribute { format: VertexFormat::Float32x2, offset: 0, shader_location: 0 },
                        VertexAttribute { format: VertexFormat::Float32x2, offset: 8, shader_location: 1 },
                        VertexAttribute { format: VertexFormat::Float32, offset: 16, shader_location: 2 },
                        VertexAttribute { format: VertexFormat::Float32, offset: 20, shader_location: 3 },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &render_shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let compute_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("particle_compute_pipeline"),
            layout: Some(&compute_pl),
            module: &compute_shader,
            entry_point: "cs_main",
            compilation_options: Default::default(),
            cache: None,
        });

        let bind_group_render = device.create_bind_group(&BindGroupDescriptor {
            label: Some("particle_render_bg"),
            layout: &render_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &global_uniform,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        let bind_group_compute = device.create_bind_group(&BindGroupDescriptor {
            label: Some("particle_compute_bg"),
            layout: &compute_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::Buffer(BufferBinding {
                        buffer: &global_uniform,
                        offset: 0,
                        size: None,
                    }),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Buffer(BufferBinding {
                        buffer: &particle_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });

        Self {
            render_pipeline,
            compute_pipeline,
            global_uniform,
            bind_group_render,
            bind_group_compute,
            particle_buffer,
        }
    }


    fn render(&self, view: &TextureView, encoder: &mut CommandEncoder) {
        // 1. Compute pass: GPU 更新粒子位置
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("particle_compute"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute_pipeline);
            pass.set_bind_group(0, &self.bind_group_compute, &[]);
            let groups = PARTICLE_COUNT.div_ceil(64);
            pass.dispatch_workgroups(groups, 1, 1);
        }

        // 2. Render pass: 绘制粒子
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("particle_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color {
                        r: 0.02,
                        g: 0.02,
                        b: 0.05,
                        a: 1.0,
                    }),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.bind_group_render, &[]);
        pass.set_vertex_buffer(0, self.particle_buffer.slice(..));
        pass.draw(0..6, 0..PARTICLE_COUNT);
    }

}

impl ParticleWallpaper {
    pub fn write_uniforms(&self, queue: &Queue, width: f32, height: f32, time: f32, dt: f32) {
        queue.write_buffer(
            &self.global_uniform,
            0,
            bytemuck::bytes_of(&GlobalUniforms {
                resolution: [width, height],
                time,
                dt,
            }),
        );
    }
}
