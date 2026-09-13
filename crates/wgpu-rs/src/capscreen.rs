//! Screenshot annotation on wasm: upload a captured frame (canvas) as a GPU
//! texture, draw line strokes over it with wgpu, and present the result on a
//! target canvas for PNG export.

use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::{Closure, JsValue, wasm_bindgen};
use web_sys::HtmlCanvasElement;

const LINE_WIDTH_PX: f32 = 3.0;
const VERTICES_PER_SEGMENT: u32 = 6;
const MAX_SEGMENTS: usize = 4096;
const CIRCLE_SEGMENTS: u32 = 48;
const UNIFORM_SIZE: u64 = 16;
const SEGMENT_SIZE: u64 = 16;

const SHADER: &str = r"
struct Uniforms {
    dims: vec2<f32>,
    width: f32,
    _pad: f32,
};

struct LineSeg {
    a: vec2<f32>,
    b: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var t_diffuse: texture_2d<f32>;
@group(0) @binding(2) var s_diffuse: sampler;
@group(0) @binding(3) var<storage, read> lines: array<LineSeg>;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_base(@builtin(vertex_index) i: u32) -> VOut {
    let x = f32(i32(i) - 1) * 0.5;
    let y = 0.5 - f32(i32(i) & 1) * 0.5;
    var o: VOut;
    o.pos = vec4<f32>(x, y, 0.0, 1.0);
    o.uv = vec2<f32>(f32(i32(i) & 1), 1.0 - f32(i32(i) & 2) * 0.5);
    return o;
}

@vertex
fn vs_line(@builtin(vertex_index) i: u32, @builtin(instance_index) seg: u32) -> VOut {
    let l = lines[seg];
    let dir_px = normalize((l.b - l.a) * u.dims);
    let half_px = vec2<f32>(-dir_px.y, dir_px.x) * (u.width * 0.5);
    let n = half_px / u.dims;
    let corner = vec2<f32>(f32(i32(i) & 1), f32((i32(i) & 2) >> 1));
    let p = mix(l.a, l.b, corner) + (corner * 2.0 - 1.0) * n;
    var o: VOut;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(0.0, 0.0);
    return o;
}

@fragment
fn fs_base(o: VOut) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, o.uv);
}

@fragment
fn fs_line(o: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.2, 0.2, 1.0);
}
";

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    base_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    segments_buf: wgpu::Buffer,
}

#[wasm_bindgen]
pub struct ScreenAnnotator {
    gpu: Rc<Gpu>,
    target: HtmlCanvasElement,
    segments: Vec<[f32; 4]>,
    segments_dirty: bool,
    /// In-progress stroke while dragging: line endpoints, or circle
    /// center + a point on the rim when [`Self::preview_circle`] is set.
    preview: Option<[f32; 4]>,
    preview_circle: bool,
}

#[wasm_bindgen]
impl ScreenAnnotator {
    /// `source` holds the captured screenshot frame; `target` receives the
    /// annotated render. From JS: `const ann = await ScreenAnnotator.new(src, dst)`.
    #[wasm_bindgen]
    pub async fn new(
        source: HtmlCanvasElement,
        target: HtmlCanvasElement,
    ) -> Result<ScreenAnnotator, JsValue> {
        let width = source.width().max(1);
        let height = source.height().max(1);
        target.set_width(width);
        target.set_height(height);

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
                label: Some("capscreen"),
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

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capscreen-shot"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::HTMLCanvasElement(source),
                origin: wgpu::Origin2d::ZERO,
                flip_y: false,
            },
            wgpu::CopyExternalImageDestInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
                color_space: wgpu::PredefinedColorSpace::Srgb,
                premultiplied_alpha: false,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("capscreen-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("capscreen-bgl"),
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
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
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
            label: Some("capscreen-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let make_pipe = |vs: &'static str, fs: &'static str, blend: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("capscreen-pipe"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let base_pipeline = make_pipe("vs_base", "fs_base", false);
        let line_pipeline = make_pipe("vs_line", "fs_line", true);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capscreen-uniforms"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let segments_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capscreen-segments"),
            size: SEGMENT_SIZE * MAX_SEGMENTS as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("capscreen-bind"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: segments_buf.as_entire_binding(),
                },
            ],
        });

        let annotator = Self {
            gpu: Rc::new(Gpu {
                device,
                queue,
                surface,
                config,
                base_pipeline,
                line_pipeline,
                bind_group,
                uniforms,
                segments_buf,
            }),
            target,
            segments: Vec::new(),
            segments_dirty: false,
            preview: None,
            preview_circle: false,
        };
        annotator.upload_uniforms();
        Ok(annotator)
    }

    /// Add a stroke from (`x1`,`y1`) to (`x2`,`y2`) in target-canvas pixels.
    pub fn add_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        if self.segments.len() < MAX_SEGMENTS {
            self.segments.push([x1, y1, x2, y2]);
            self.segments_dirty = true;
        }
    }

    /// Add a circle outline at (`cx`,`cy`) with `radius` px, rendered as a
    /// closed polyline of [`CIRCLE_SEGMENTS`] line segments.
    pub fn add_circle(&mut self, cx: f32, cy: f32, radius: f32) {
        let step = 2.0 * std::f32::consts::PI / CIRCLE_SEGMENTS as f32;
        let pt = |i: u32| {
            let a = step * i as f32;
            (cx + radius * a.cos(), cy + radius * a.sin())
        };
        for i in 0..CIRCLE_SEGMENTS {
            let (x1, y1) = pt(i);
            let (x2, y2) = pt(i + 1);
            self.add_line(x1, y1, x2, y2);
        }
    }

    pub fn undo_line(&mut self) {
        self.segments.pop();
        self.segments_dirty = true;
    }

    pub fn clear_lines(&mut self) {
        self.segments.clear();
        self.segments_dirty = true;
    }

    /// Show a live in-progress stroke while dragging. With `circle` set,
    /// (`x1`,`y1`) is the center and (`x2`,`y2`) a point on the rim.
    pub fn set_preview(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, circle: bool) {
        self.preview = Some([x1, y1, x2, y2]);
        self.preview_circle = circle;
    }

    pub fn clear_preview(&mut self) {
        self.preview = None;
    }

    pub fn line_count(&self) -> u32 {
        self.segments.len() as u32
    }

    /// Render base screenshot + all strokes (plus the live preview, if any)
    /// onto the target canvas.
    pub fn render(&self) -> Result<(), JsValue> {
        let preview_segs = self.preview_segments();
        if self.segments_dirty || !preview_segs.is_empty() {
            self.upload_segments(&preview_segs);
        }
        let gpu = &self.gpu;
        let frame = gpu
            .surface
            .get_current_texture()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let view = frame.texture.create_view(&Default::default());
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("capscreen-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(Default::default()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &gpu.bind_group, &[]);
            pass.set_pipeline(&gpu.base_pipeline);
            pass.draw(0..3, 0..1);
            let total = self.segments.len() + preview_segs.len();
            if total > 0 {
                pass.set_pipeline(&gpu.line_pipeline);
                pass.draw(0..VERTICES_PER_SEGMENT, 0..total as u32);
            }
        }
        gpu.queue.submit([enc.finish()]);
        frame.present();
        Ok(())
    }

    /// Export the annotated target canvas as a PNG `Blob`.
    pub async fn to_png(&self) -> Result<JsValue, JsValue> {
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let resolve = resolve.unchecked_into::<js_sys::Function>();
            let reject = reject.unchecked_into::<js_sys::Function>();
            let cb = Closure::wrap(Box::new(move |blob: js_sys::Object| {
                resolve.call1(&JsValue::NULL, &blob).ok();
            }) as Box<dyn FnMut(js_sys::Object)>);
            let cb_fn = cb.into_js_value().unchecked_into::<js_sys::Function>();
            if self.target.to_blob(&cb_fn).is_err() {
                reject
                    .call1(&JsValue::NULL, &JsValue::from_str("to_blob failed"))
                    .ok();
            }
        });
        wasm_bindgen_futures::JsFuture::from(promise).await
    }
}

impl ScreenAnnotator {
    fn upload_uniforms(&self) {
        let mut bytes = Vec::with_capacity(UNIFORM_SIZE as usize);
        bytes.extend_from_slice(&(self.gpu.config.width as f32).to_le_bytes());
        bytes.extend_from_slice(&(self.gpu.config.height as f32).to_le_bytes());
        bytes.extend_from_slice(&LINE_WIDTH_PX.to_le_bytes());
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        self.gpu.queue.write_buffer(&self.gpu.uniforms, 0, &bytes);
    }

    /// The in-progress stroke expanded to line segments (1 for a line,
    /// [`CIRCLE_SEGMENTS`] for a circle).
    fn preview_segments(&self) -> Vec<[f32; 4]> {
        let Some([x1, y1, x2, y2]) = self.preview else {
            return Vec::new();
        };
        if !self.preview_circle {
            return vec![[x1, y1, x2, y2]];
        }
        let radius = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
        let step = 2.0 * std::f32::consts::PI / CIRCLE_SEGMENTS as f32;
        (0..CIRCLE_SEGMENTS)
            .map(|i| {
                let (ax, ay) = circle_pt(x1, y1, radius, step * i as f32);
                let (bx, by) = circle_pt(x1, y1, radius, step * (i + 1) as f32);
                [ax, ay, bx, by]
            })
            .collect()
    }

    fn upload_segments(&self, preview_segs: &[[f32; 4]]) {
        let count = (self.segments.len() + preview_segs.len()).min(MAX_SEGMENTS);
        let mut bytes = Vec::with_capacity(SEGMENT_SIZE as usize * count);
        for s in self.segments.iter().chain(preview_segs).take(count) {
            for v in s {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        self.gpu
            .queue
            .write_buffer(&self.gpu.segments_buf, 0, &bytes);
    }
}

fn circle_pt(cx: f32, cy: f32, radius: f32, angle: f32) -> (f32, f32) {
    (cx + radius * angle.cos(), cy + radius * angle.sin())
}
