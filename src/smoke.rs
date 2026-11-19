//! Bluish smoke ring around the quest card.
//!
//! wgpu is the GPU library iced already renders with. No particle crate can
//! draw into that same pass, so the wisps are a small shader on that device.

use iced::Rectangle;
use iced::widget::shader::{self, Viewport};
use iced::wgpu;

pub const PAD: f32 = 96.0;

pub struct Smoke {
    pub time: f32,
}

#[derive(Debug)]
pub struct Primitive {
    time: f32,
}

pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    vertices: wgpu::Buffer,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    time: f32,
    pad: f32,
    resolution: [f32; 2],
}

impl shader::Program<super::Message> for Smoke {
    type State = ();
    type Primitive = Primitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: iced::mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        Primitive { time: self.time }
    }
}

impl shader::Primitive for Primitive {
    type Pipeline = Pipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        let uniforms = Uniforms {
            time: self.time,
            pad: PAD,
            resolution: [bounds.width, bounds.height],
        };
        queue.write_buffer(&pipeline.uniform, 0, bytemuck::bytes_of(&uniforms));
    }

    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, pipeline.vertices.slice(..));
        pass.draw(0..6, 0..1);
        true
    }
}

impl shader::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("smoke"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("smoke-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("smoke-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("smoke-bind"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });

        // clip x, clip y, uv x, uv y. Two triangles, uv origin at the top left.
        let vertex_data: [[f32; 4]; 6] = [
            [-1.0, -1.0, 0.0, 1.0],
            [1.0, -1.0, 1.0, 1.0],
            [-1.0, 1.0, 0.0, 0.0],
            [-1.0, 1.0, 0.0, 0.0],
            [1.0, -1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 0.0],
        ];
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("smoke-vertices"),
            size: std::mem::size_of_val(&vertex_data) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        vertices
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::bytes_of(&vertex_data));
        vertices.unmap();

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("smoke-pipeline-layout"),
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("smoke-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group,
            uniform,
            vertices,
        }
    }
}

const SHADER: &str = r#"
struct Uniforms {
    time: f32,
    pad: f32,
    resolution: vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> VsOut {
    var out: VsOut;
    out.clip = vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let q = vec2<f32>(
        dot(p, vec2<f32>(127.1, 311.7)),
        dot(p, vec2<f32>(269.5, 183.3)),
    );
    return fract(sin(q) * 43758.5453) * 2.0 - 1.0;
}

// Gradient noise. Value noise was leaving a visible grid of soft blocks.
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let n00 = dot(hash2(i), f);
    let n10 = dot(hash2(i + vec2<f32>(1.0, 0.0)), f - vec2<f32>(1.0, 0.0));
    let n01 = dot(hash2(i + vec2<f32>(0.0, 1.0)), f - vec2<f32>(0.0, 1.0));
    let n11 = dot(hash2(i + vec2<f32>(1.0, 1.0)), f - vec2<f32>(1.0, 1.0));
    return mix(mix(n00, n10, u.x), mix(n01, n11, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var q = p;
    for (var i = 0; i < 4; i = i + 1) {
        v = v + a * noise(q);
        q = q * 2.02 + vec2<f32>(1.7, 9.2);
        a = a * 0.5;
    }
    return v;
}

// Two warps so the wisps curl instead of sitting in cells.
fn smoke(p: vec2<f32>) -> f32 {
    let q = vec2<f32>(
        fbm(p),
        fbm(p + vec2<f32>(5.2, 1.3)),
    );
    let r = vec2<f32>(
        fbm(p + 1.9 * q + vec2<f32>(1.7, 9.2)),
        fbm(p + 1.9 * q + vec2<f32>(8.3, 2.8)),
    );
    return fbm(p + 1.6 * r) * 0.5 + 0.5;
}

fn sd_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let px = (in.uv - vec2<f32>(0.5, 0.5)) * u.resolution;
    let half = u.resolution * 0.5 - vec2<f32>(u.pad, u.pad);
    let d = sd_box(px, half, 22.0);

    let t = u.time;
    let flow = px / 46.0 + vec2<f32>(t * 0.07, -t * 0.04);
    let n = smoke(flow);
    let fine = smoke(flow * 1.8 - vec2<f32>(t * 0.05, t * 0.08) + vec2<f32>(3.1, 2.4));

    // Soft plume outside the card. Fade to nothing before the layer edge,
    // or the compositor clips the haze into a hard square.
    let falloff = exp(-max(d, 0.0) / 26.0);
    let outside = smoothstep(-2.0, 16.0, d);
    let inside_pad = smoothstep(u.pad, 14.0, d);
    let edge = u.resolution * 0.5 - abs(px);
    let frame = smoothstep(0.0, 48.0, min(edge.x, edge.y));
    let body = smoothstep(0.38, 0.72, n);
    let thread = smoothstep(0.55, 0.82, fine);
    var alpha = falloff * outside * inside_pad * frame * (body * 0.62 + thread * 0.38);
    alpha = clamp(alpha, 0.0, 0.62);

    let deep = vec3<f32>(0.16, 0.38, 0.92);
    let bright = vec3<f32>(0.72, 0.92, 1.0);
    let col = mix(deep, bright, clamp(fine, 0.0, 1.0));
    return vec4<f32>(col, alpha);
}
"#;
