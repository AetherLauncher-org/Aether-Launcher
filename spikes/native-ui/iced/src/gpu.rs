//! The cube grid as an iced shader widget: iced hands over its wgpu device and the window's
//! target; the widget makes its own render pass with its own depth buffer.

use std::borrow::Cow;

use iced::wgpu;
use iced::widget::shader::{self, Viewport};
use iced::{Rectangle, mouse};
use native_ui_spike_common::cube;
use native_ui_spike_common::stats;

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The widget: only the angle changes from frame to frame.
pub struct CubeView {
    pub angle: f32,
}

impl<Message> shader::Program<Message> for CubeView {
    type State = ();
    type Primitive = CubePrimitive;

    fn draw(&self, _state: &(), _cursor: mouse::Cursor, _bounds: Rectangle) -> CubePrimitive {
        CubePrimitive { angle: self.angle }
    }
}

#[derive(Debug)]
pub struct CubePrimitive {
    angle: f32,
}

impl shader::Primitive for CubePrimitive {
    type Pipeline = CubePipeline;

    fn prepare(
        &self,
        pipeline: &mut CubePipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        stats::record_frame();
        let full = viewport.physical_size();
        pipeline.ensure_depth(device, full.width.max(1), full.height.max(1));
        let aspect = bounds.width / bounds.height.max(1.0);
        queue.write_buffer(&pipeline.uniform, 0, &cube::uniforms(self.angle, aspect));
    }

    fn render(
        &self,
        pipeline: &CubePipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip: &Rectangle<u32>,
    ) {
        let Some(depth) = &pipeline.depth else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cube grid"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(
            clip.x as f32,
            clip.y as f32,
            clip.width as f32,
            clip.height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, pipeline.vertices.slice(..));
        pass.set_vertex_buffer(1, pipeline.instances.slice(..));
        pass.draw(0..36, 0..pipeline.instance_count);
    }
}

struct Depth {
    size: (u32, u32),
    view: wgpu::TextureView,
}

pub struct CubePipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    vertices: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_count: u32,
    depth: Option<Depth>,
}

impl CubePipeline {
    /// The depth buffer has to be as large as the window's target, because a render pass takes
    /// attachments of one size. Made again only when the window changes size.
    fn ensure_depth(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.depth.as_ref().is_some_and(|d| d.size == (width, height)) {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cube depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth = Some(Depth {
            size: (width, height),
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
        });
    }
}

impl shader::Pipeline for CubePipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        use wgpu::util::DeviceExt;

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
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
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
                    wgpu::VertexBufferLayout {
                        array_stride: cube::VERTEX_STRIDE,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: cube::INSTANCE_STRIDE,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![2 => Float32x4],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group,
            uniform,
            vertices,
            instances,
            instance_count: instance_data.len() as u32,
            depth: None,
        }
    }
}
