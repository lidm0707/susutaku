use crate::anim::Animation;
use std::fmt;

#[derive(Debug)]
pub enum RenderError {
    Surface(wgpu::SurfaceError),
    Request(wgpu::RequestDeviceError),
    CreateSurface(wgpu::CreateSurfaceError),
    NoAdapter,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Surface(e) => write!(f, "surface: {e}"),
            Self::Request(e) => write!(f, "device: {e}"),
            Self::CreateSurface(e) => write!(f, "create surface: {e}"),
            Self::NoAdapter => write!(f, "no suitable adapter"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<wgpu::SurfaceError> for RenderError {
    fn from(e: wgpu::SurfaceError) -> Self {
        Self::Surface(e)
    }
}

impl From<wgpu::RequestDeviceError> for RenderError {
    fn from(e: wgpu::RequestDeviceError) -> Self {
        Self::Request(e)
    }
}

impl From<wgpu::CreateSurfaceError> for RenderError {
    fn from(e: wgpu::CreateSurfaceError) -> Self {
        Self::CreateSurface(e)
    }
}

const CLEAR_BASE: f64 = 0.1;
const CLEAR_WOBBLE: f64 = 0.05;

const SHADER: &str = r"
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) color: vec3<f32> };

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VOut {
    let x = f32(i32(i) - 1) * 0.5;
    let y = f32(i32(i) & 1) * 0.5 - 0.25;
    var o: VOut;
    o.pos = vec4<f32>(x, y, 0.0, 1.0);
    o.color = vec3<f32>(f32(i & 1u), f32(i & 2u) * 0.5, 1.0 - f32(i) * 0.25);
    return o;
}

@fragment
fn fs_main(o: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(o.color, 1.0);
}
";

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_cfg: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    anim: Animation,
}

pub struct RendererConfig {
    pub surface_target: wgpu::SurfaceTarget<'static>,
    pub width: u32,
    pub height: u32,
    pub speed: f32,
}

impl Renderer {
    pub async fn new(cfg: RendererConfig) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(cfg.surface_target)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| RenderError::NoAdapter)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("wgpu-rs"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        let caps = surface.get_capabilities(&adapter);
        let texture_format = caps.formats[0];
        let surface_cfg = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: texture_format,
            width: cfg.width,
            height: cfg.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_cfg);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("anim-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("anim-pipeline"),
            layout: None,
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
                    format: texture_format,
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

        Ok(Self {
            device,
            queue,
            surface,
            surface_cfg,
            pipeline,
            anim: Animation::new(cfg.speed),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_cfg.width = width;
        self.surface_cfg.height = height;
        self.surface.configure(&self.device, &self.surface_cfg);
    }

    pub fn render_frame(&mut self, dt: f32) -> Result<(), RenderError> {
        self.anim.tick(dt);
        let phase = f64::from(self.anim.phase());
        let wobble = phase.sin() * CLEAR_WOBBLE;
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: CLEAR_BASE + wobble,
                            g: CLEAR_BASE,
                            b: CLEAR_BASE - wobble,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([enc.finish()]);
        frame.present();
        Ok(())
    }
}
