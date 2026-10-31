//! The cube grid for Slint: drawn into a texture of its own (with its own depth buffer) using
//! the device and queue Slint hands over, then imported as a `slint::Image`. Slint 1.18's wgpu
//! 30 integration is the same wgpu version as the egui spike, so the pipeline is the same.

use std::borrow::Cow;

use native_ui_spike_common::cube;
use slint::wgpu_30::wgpu;
use wgpu::util::DeviceExt;

const COLOUR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The size of the picture. Slint scales it to the space it has (`image-fit: contain`).
pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 540;

pub struct Scene {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    vertices: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_count: u32,
    colour: wgpu::Texture,
    colour_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
}

impl Scene {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cube shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(cube::SHADER)),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cube uniforms"),
            size: cube::UNIFORM_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cube layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cube bind group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cube pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let instance_data = cube::instances(cube::GRID_SIDE);
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertices"),
            contents: &cube::as_bytes(&cube::vertices()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube instances"),
            contents: &cube::as_bytes(&instance_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cube pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: cube::VERTEX_STRIDE,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: cube::INSTANCE_STRIDE,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![2 => Float32x4],
                    }),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOUR,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let make = |format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("cube target"),
                size: wgpu::Extent3d {
                    width: WIDTH,
                    height: HEIGHT,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let colour = make(
            COLOUR,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = make(DEPTH, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let colour_view = colour.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            pipeline,
            bind_group,
            uniform,
            vertices,
            instances,
            instance_count: instance_data.len() as u32,
            colour,
            colour_view,
            depth_view,
        }
    }

    /// The texture the grid is drawn into, to be imported as a `slint::Image` once.
    pub fn texture(&self) -> wgpu::Texture {
        self.colour.clone()
    }

    /// Draws the grid turned by `angle` into the texture.
    pub fn render(&self, device: &wgpu::Device, queue: &wgpu::Queue, angle: f32) {
        let aspect = WIDTH as f32 / HEIGHT as f32;
        queue.write_buffer(&self.uniform, 0, &cube::uniforms(angle, aspect));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("cube encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cube grid"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.colour_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.06,
                            g: 0.07,
                            b: 0.09,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.draw(0..36, 0..self.instance_count);
        }
        queue.submit([encoder.finish()]);
    }
}
