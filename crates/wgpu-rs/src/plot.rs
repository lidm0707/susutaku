//! Math plot rendering on wasm: grid, axes and series polylines drawn with
//! wgpu onto an HTMLCanvasElement. From JS: `const p = await Plotter.new(canvas)`.

use wasm_bindgen::prelude::{JsValue, wasm_bindgen};
use web_sys::HtmlCanvasElement;

const MAX_SEGMENTS: usize = 8192;
const FLOATS_PER_SEGMENT: usize = 8;
const UNIFORM_SIZE: u64 = 16;
const SEGMENT_SIZE: u64 = (FLOATS_PER_SEGMENT * 4) as u64;
const TARGET_GRID_LINES: f64 = 10.0;
const LINE_WIDTH_PX: f32 = 2.0;
const CLIP_CLAMP: f64 = 1.05;
const DEFAULT_VIEW: [f64; 4] = [-10.0, 10.0, -5.0, 5.0];

const BG: wgpu::Color = wgpu::Color {
    r: 0.063,
    g: 0.063,
    b: 0.078,
    a: 1.0,
};
const GRID_COLOR: [f32; 4] = [0.16, 0.16, 0.20, 1.0];
const AXIS_COLOR: [f32; 4] = [0.42, 0.42, 0.48, 1.0];
const PALETTE: [[f32; 4]; 6] = [
    [0.42, 0.68, 1.0, 1.0],
    [1.0, 0.55, 0.35, 1.0],
    [0.55, 0.90, 0.55, 1.0],
    [0.90, 0.55, 0.95, 1.0],
    [1.0, 0.85, 0.35, 1.0],
    [0.45, 0.90, 0.85, 1.0],
];

const SHADER: &str = r"
struct Uniforms {
    dims: vec2<f32>,
    width: f32,
    _pad: f32,
};

struct Seg {
    a: vec2<f32>,
    b: vec2<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> segs: array<Seg>;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32, @builtin(instance_index) s: u32) -> VOut {
    let seg = segs[s];
    let dir_px = normalize((seg.b - seg.a) * u.dims);
    let half_px = vec2<f32>(-dir_px.y, dir_px.x) * (u.width * 0.5);
    let n = half_px / u.dims;
    let corner = vec2<f32>(f32(i32(i) & 1), f32((i32(i) & 2) >> 1));
    let p = mix(seg.a, seg.b, corner) + (corner * 2.0 - 1.0) * n;
    var o: VOut;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.color = seg.color;
    return o;
}

@fragment
fn fs_main(o: VOut) -> @location(0) vec4<f32> {
    return o.color;
}
";

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    segs_buf: wgpu::Buffer,
}

#[wasm_bindgen]
pub struct Plotter {
    gpu: Gpu,
    target: HtmlCanvasElement,
    view: [f64; 4],
    series: Vec<Vec<f64>>,
}

#[wasm_bindgen]
impl Plotter {
    #[wasm_bindgen]
    pub async fn new(target: HtmlCanvasElement) -> Result<Plotter, JsValue> {
        let width = target.width().max(1);
        let height = target.height().max(1);
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(target.clone()))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| JsValue::from_str("no suitable gpu adapter"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("plot"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("plot-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("plot-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("plot-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("plot-pipe"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("plot-uniforms"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let segs_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("plot-segs"),
            size: SEGMENT_SIZE * MAX_SEGMENTS as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("plot-bind"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: segs_buf.as_entire_binding(),
                },
            ],
        });

        Ok(Self {
            gpu: Gpu {
                device,
                queue,
                surface,
                config,
                pipeline,
                bind_group,
                uniforms,
                segs_buf,
            },
            target,
            view: DEFAULT_VIEW,
            series: Vec::new(),
        })
    }

    /// View window: x_min, x_max, y_min, y_max in data coordinates.
    pub fn set_view(&mut self, x_min: f64, x_max: f64, y_min: f64, y_max: f64) {
        self.view = [x_min, x_max, y_min, y_max];
    }

    /// One polyline as flat [x0, y0, x1, y1, …] in data coordinates.
    pub fn add_series(&mut self, points: Vec<f64>) {
        self.series.push(points);
    }

    pub fn clear_series(&mut self) {
        self.series.clear();
    }

    pub fn render(&mut self) {
        let width = self.target.width().max(1);
        let height = self.target.height().max(1);
        if self.gpu.config.width != width || self.gpu.config.height != height {
            self.gpu.config.width = width;
            self.gpu.config.height = height;
            self.gpu
                .surface
                .configure(&self.gpu.device, &self.gpu.config);
        }
        let segs = self.build_segments();
        let bytes: Vec<u8> = segs.iter().flat_map(|f| f.to_ne_bytes()).collect();
        self.gpu.queue.write_buffer(&self.gpu.segs_buf, 0, &bytes);
        self.upload_uniforms(width, height);

        let frame = match self.gpu.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("plot"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BG),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.gpu.pipeline);
            pass.set_bind_group(0, &self.gpu.bind_group, &[]);
            pass.draw(0..6, 0..(segs.len() / FLOATS_PER_SEGMENT) as u32);
        }
        self.gpu.queue.submit([enc.finish()]);
        frame.present();
    }
}

impl Plotter {
    fn upload_uniforms(&self, width: u32, height: u32) {
        let mut bytes = [0u8; UNIFORM_SIZE as usize];
        bytes[0..4].copy_from_slice(&(width as f32).to_ne_bytes());
        bytes[4..8].copy_from_slice(&(height as f32).to_ne_bytes());
        bytes[8..12].copy_from_slice(&LINE_WIDTH_PX.to_ne_bytes());
        self.gpu.queue.write_buffer(&self.gpu.uniforms, 0, &bytes);
    }

    /// Grid + axes + all series as flat clip-space segments
    /// ([ax, ay, bx, by, r, g, b, a] each), capped at [`MAX_SEGMENTS`].
    fn build_segments(&self) -> Vec<f32> {
        let mut segs: Vec<f32> = Vec::new();
        let [x0, x1, y0, y1] = self.view;

        let gx = nice_step(x1 - x0);
        let gy = nice_step(y1 - y0);
        let mut x = (x0 / gx).ceil() * gx;
        while x <= x1 {
            push(x, y0, x, y1, grid_color(x, gx), &mut segs, self.view);
            x += gx;
        }
        let mut y = (y0 / gy).ceil() * gy;
        while y <= y1 {
            push(x0, y, x1, y, grid_color(y, gy), &mut segs, self.view);
            y += gy;
        }

        for (i, pts) in self.series.iter().enumerate() {
            let color = PALETTE[i % PALETTE.len()];
            let mut prev: Option<[f64; 2]> = None;
            for pair in pts.chunks_exact(2) {
                if let Some(p) = prev {
                    push(p[0], p[1], pair[0], pair[1], color, &mut segs, self.view);
                }
                prev = Some([pair[0], pair[1]]);
                if segs.len() >= MAX_SEGMENTS * FLOATS_PER_SEGMENT {
                    return segs;
                }
            }
        }
        segs
    }
}

fn push(ax: f64, ay: f64, bx: f64, by: f64, color: [f32; 4], segs: &mut Vec<f32>, view: [f64; 4]) {
    if segs.len() >= MAX_SEGMENTS * FLOATS_PER_SEGMENT {
        return;
    }
    let [x0, x1, y0, y1] = view;
    let (ax, ay) = to_clip(ax, ay, x0, x1, y0, y1);
    let (bx, by) = to_clip(bx, by, x0, x1, y0, y1);
    if !(ax.is_finite() && ay.is_finite() && bx.is_finite() && by.is_finite()) {
        return;
    }
    if ax == bx && ay == by {
        return;
    }
    segs.extend_from_slice(&[ax, ay, bx, by, color[0], color[1], color[2], color[3]]);
}

/// Non-finite values clamp to just outside the frame, keeping polylines
/// continuous across the visible range.
fn to_clip(x: f64, y: f64, x0: f64, x1: f64, y0: f64, y1: f64) -> (f32, f32) {
    let cx = ((x - x0) / (x1 - x0)).clamp(-CLIP_CLAMP, 1.0 + CLIP_CLAMP);
    let cy = ((y - y0) / (y1 - y0)).clamp(-CLIP_CLAMP, 1.0 + CLIP_CLAMP);
    ((cx * 2.0 - 1.0) as f32, (cy * 2.0 - 1.0) as f32)
}

fn grid_color(pos: f64, step: f64) -> [f32; 4] {
    if pos.abs() <= step * 0.5 {
        AXIS_COLOR
    } else {
        GRID_COLOR
    }
}

fn nice_step(range: f64) -> f64 {
    if !range.is_finite() || range <= 0.0 {
        return 1.0;
    }
    let raw = range / TARGET_GRID_LINES;
    let mag = 10f64.powf(raw.abs().log10().floor());
    let norm = raw / mag;
    let mult = if norm < 1.5 {
        1.0
    } else if norm < 3.5 {
        2.0
    } else if norm < 7.5 {
        5.0
    } else {
        10.0
    };
    mult * mag
}
