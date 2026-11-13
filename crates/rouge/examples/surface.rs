use rouge::AppError;
use rouge::prelude::*;
use rouge::render::{
    BufferDesc, BufferRef, BufferSetId, Frame, SharedBuffer, Sharing, SurfaceConfig, SurfaceCx,
    SurfaceEvent, SurfaceOptions, SurfaceProducer, surface_channel,
};
use std::collections::VecDeque;
use std::thread::spawn;
use std::time::Instant;
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BufferUsages, Color as ClearColor, ColorTargetState,
    ColorWrites, CommandEncoderDescriptor, FragmentState, LoadOp, MultisampleState, Operations,
    PipelineCompilationOptions, PrimitiveState, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource, StoreOp,
    TextureFormat, TextureViewDescriptor, VertexState,
};

const PLASMA_WGSL: &str = "
@group(0) @binding(0) var<uniform> time: vec4<f32>;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VsOut;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = time.x;
    let p = in.uv * 6.0;
    let v = sin(p.x + t) + sin(p.y * 1.3 - t * 0.7) + sin(length(p - vec2<f32>(3.0)) * 1.7 - t * 1.4);
    let color = 0.5 + 0.5 * cos(vec3<f32>(0.0, 2.1, 4.2) + v + t * 0.3);
    return vec4<f32>(color * 0.85, 1.0);
}
";

fn main() -> Result<(), AppError> {
    let (producer, source) = surface_channel(SurfaceOptions::default());
    spawn(move || Plasma::new(producer).run());
    let options = WindowOptions::default()
        .title("Surface")
        .size(720.0, 480.0)
        .background(Color::rgb(0x10111a));
    App::new().window(options, move || view(source)).run()
}

fn view(source: SurfaceSource) -> impl View {
    div().size_full().p(24.0).child(
        div()
            .relative()
            .size_full()
            .child(
                surface(source)
                    .size_full()
                    .rounded(20.0)
                    .fallback(|error| text(error.to_string()).color(Color::WHITE)),
            )
            .child(
                div()
                    .absolute()
                    .left(32.0)
                    .top(32.0)
                    .p(20.0)
                    .rounded(28.0)
                    .backdrop(Glass::new().blur(3.0).edge_width(18.0))
                    .child(
                        text("Glass over a live GPU surface")
                            .text_size(20.0)
                            .bold()
                            .color(Color::WHITE),
                    ),
            ),
    )
}

struct Chain {
    set: BufferSetId,
    buffers: Vec<SharedBuffer>,
    free: VecDeque<u32>,
}

struct Plasma {
    producer: SurfaceProducer,
    cx: Option<SurfaceCx>,
    config: Option<SurfaceConfig>,
    chain: Option<Chain>,
    pipeline: Option<RenderPipeline>,
    started: Instant,
}

impl Plasma {
    fn new(producer: SurfaceProducer) -> Self {
        Plasma {
            producer,
            cx: None,
            config: None,
            chain: None,
            pipeline: None,
            started: Instant::now(),
        }
    }

    fn run(mut self) {
        while let Some(event) = self.producer.blocking_recv() {
            match event {
                SurfaceEvent::Attached(cx) => self.cx = Some(*cx),
                SurfaceEvent::Configure(config) => {
                    self.config = Some(config);
                    if let Err(error) = self.allocate() {
                        self.producer.fail(error);
                        return;
                    }
                    if config.visible {
                        self.producer.request_frame();
                    }
                }
                SurfaceEvent::FrameRequest { .. } => {
                    self.render();
                    self.producer.request_frame();
                }
                SurfaceEvent::Released(buffer) => {
                    if let Some(chain) = self.chain.as_mut().filter(|c| c.set == buffer.set) {
                        chain.free.push_back(buffer.index);
                    }
                }
                SurfaceEvent::Detached => return,
                SurfaceEvent::Presented { .. } | SurfaceEvent::Discarded(_) => {}
            }
        }
    }

    fn allocate(&mut self) -> Result<(), SurfaceError> {
        let (Some(cx), Some(config)) = (&self.cx, self.config) else {
            return Ok(());
        };
        if config.width == 0 || config.height == 0 {
            return Ok(());
        }
        let desc = BufferDesc::new(config.width, config.height);
        let buffers = cx
            .allocate(&desc.sharing(Sharing::Native))
            .or_else(|_| cx.allocate(&desc))?;
        let set = self.producer.register(&buffers);
        if let Some(old) = self.chain.take() {
            self.producer.unregister(old.set);
        }
        let free = (0..buffers.len() as u32).collect();
        self.chain = Some(Chain { set, buffers, free });
        Ok(())
    }

    fn render(&mut self) {
        let (Some(cx), Some(chain)) = (&self.cx, &mut self.chain) else {
            return;
        };
        let Some(index) = chain.free.pop_front() else {
            return;
        };
        let device = cx.device();
        let pipeline = self
            .pipeline
            .get_or_insert_with(|| plasma_pipeline(cx, TextureFormat::Bgra8Unorm));
        let time = [self.started.elapsed().as_secs_f32(), 0.0, 0.0, 0.0];
        let bytes: Vec<u8> = time.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let uniform = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("plasma time"),
            contents: &bytes,
            usage: BufferUsages::UNIFORM,
        });
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("plasma"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let view = chain.buffers[index as usize]
            .texture()
            .create_view(&TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("plasma"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(ClearColor::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        cx.queue().submit([encoder.finish()]);
        let seq = self.config.map_or(0, |config| config.seq);
        let set = chain.set;
        self.producer
            .submit(Frame::new(BufferRef { set, index }).config(seq));
    }
}

fn plasma_pipeline(cx: &SurfaceCx, format: TextureFormat) -> RenderPipeline {
    let module = cx.device().create_shader_module(ShaderModuleDescriptor {
        label: Some("plasma"),
        source: ShaderSource::Wgsl(PLASMA_WGSL.into()),
    });
    cx.device()
        .create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("plasma"),
            layout: None,
            vertex: VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
}
