use crate::gpu::begin_pass;
use crate::surface::{
    BufferDesc, Frame, SharedBuffer, Sharing, SurfaceConfig, SurfaceCx, SurfaceError, SurfaceEvent,
    SurfaceId, SurfaceProducer, SurfaceSource, surface_channel,
};
use bytemuck::{Pod, Zeroable, bytes_of};
use thiserror::Error;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, Buffer, BufferDescriptor, BufferUsages,
    Color as ClearColor, ColorTargetState, ColorWrites, CommandEncoderDescriptor, Device,
    FragmentState, LoadOp, MultisampleState, PipelineCompilationOptions, PrimitiveState,
    RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource, TextureFormat,
    TextureViewDescriptor, VertexState,
};

const PATTERN_WGSL: &str = include_str!("shaders/test_pattern.wgsl");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Solid([u8; 4]),
    Quadrants {
        top_left: [u8; 4],
        top_right: [u8; 4],
        bottom_left: [u8; 4],
        bottom_right: [u8; 4],
    },
}

impl Pattern {
    pub const fn quadrants(colors: [[u8; 4]; 4]) -> Pattern {
        let [top_left, top_right, bottom_left, bottom_right] = colors;
        Pattern::Quadrants {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
        }
    }

    fn colors(self) -> [[u8; 4]; 4] {
        match self {
            Pattern::Solid(color) => [color; 4],
            Pattern::Quadrants {
                top_left,
                top_right,
                bottom_left,
                bottom_right,
            } => [top_left, top_right, bottom_left, bottom_right],
        }
    }
}

#[derive(Debug, Error)]
pub enum TestSurfaceError {
    #[error("the surface has no size yet")]
    NoSize,
    #[error("every buffer is still held by the compositor")]
    NoFreeBuffer,
    #[error(transparent)]
    Surface(#[from] SurfaceError),
}

#[derive(Clone, Copy, Debug)]
pub struct TestSurfaceBuilder {
    desc: BufferDesc,
    size: Option<(u32, u32)>,
}

impl Default for TestSurfaceBuilder {
    fn default() -> Self {
        TestSurfaceBuilder {
            desc: BufferDesc::new(0, 0),
            size: None,
        }
    }
}

impl TestSurfaceBuilder {
    pub fn buffers(mut self, buffers: u32) -> Self {
        self.desc = self.desc.count(buffers);
        self
    }

    pub fn sharing(mut self, sharing: Sharing) -> Self {
        self.desc = self.desc.sharing(sharing);
        self
    }

    pub fn format(mut self, format: TextureFormat) -> Self {
        self.desc = self.desc.format(format);
        self
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = Some((width, height));
        self
    }

    pub fn build(self) -> (TestSurface, SurfaceSource) {
        let (producer, source) = surface_channel();
        let surface = TestSurface {
            producer,
            settings: self,
            config: None,
            painter: None,
            detached: false,
            log: Vec::new(),
        };
        (surface, source)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PatternUniform {
    colors: [[f32; 4]; 4],
    flip: [f32; 4],
}

struct Painter {
    pipeline: RenderPipeline,
    uniform: Buffer,
    bind_group: BindGroup,
}

impl Painter {
    fn new(device: &Device, format: TextureFormat) -> Painter {
        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("rouge test pattern"),
            source: ShaderSource::Wgsl(PATTERN_WGSL.into()),
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("rouge test pattern"),
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
        });
        let uniform = device.create_buffer(&BufferDescriptor {
            label: Some("rouge test pattern"),
            size: size_of::<PatternUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("rouge test pattern"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        Painter {
            pipeline,
            uniform,
            bind_group,
        }
    }
}

pub struct TestSurface {
    producer: SurfaceProducer,
    settings: TestSurfaceBuilder,
    config: Option<SurfaceConfig>,
    painter: Option<Painter>,
    detached: bool,
    log: Vec<SurfaceEvent>,
}

impl TestSurface {
    pub fn builder() -> TestSurfaceBuilder {
        TestSurfaceBuilder::default()
    }

    pub fn new() -> (TestSurface, SurfaceSource) {
        TestSurfaceBuilder::default().build()
    }

    pub fn pump(&mut self) -> Result<&[SurfaceEvent], TestSurfaceError> {
        self.log.clear();
        while let Some(event) = self.producer.try_recv() {
            match event {
                SurfaceEvent::Configure(config) => self.config = Some(config),
                SurfaceEvent::Detached => self.detached = true,
                SurfaceEvent::Attached | SurfaceEvent::FrameRequest { .. } => {}
            }
            self.log.push(event);
        }
        self.configure()?;
        Ok(&self.log)
    }

    pub fn present(&mut self, pattern: Pattern) -> Result<(), TestSurfaceError> {
        self.present_frame(pattern, |frame| frame)
    }

    pub fn present_frame(
        &mut self,
        pattern: Pattern,
        frame: impl FnOnce(Frame) -> Frame,
    ) -> Result<(), TestSurfaceError> {
        self.render(pattern, false, frame)
    }

    pub fn present_flipped(
        &mut self,
        pattern: Pattern,
        frame: impl FnOnce(Frame) -> Frame,
    ) -> Result<(), TestSurfaceError> {
        self.render(pattern, true, frame)
    }

    pub fn request_frame(&self) {
        self.producer.request_frame();
    }

    pub fn fail(&self, error: SurfaceError) {
        self.producer.fail(error);
    }

    pub fn id(&self) -> SurfaceId {
        self.producer.id()
    }

    pub fn config(&self) -> Option<SurfaceConfig> {
        self.config
    }

    pub fn cx(&self) -> Option<&SurfaceCx> {
        self.producer.cx()
    }

    pub fn free_buffers(&self) -> usize {
        self.producer.free_buffers()
    }

    pub fn buffers(&self) -> &[SharedBuffer] {
        self.producer.buffers()
    }

    pub fn is_detached(&self) -> bool {
        self.detached
    }

    fn size(&self) -> Option<(u32, u32)> {
        self.settings.size.or_else(|| {
            let config = self.config?;
            (config.width > 0 && config.height > 0).then_some((config.width, config.height))
        })
    }

    fn configure(&mut self) -> Result<(), TestSurfaceError> {
        let (Some(_), Some((width, height))) = (self.producer.cx(), self.size()) else {
            return Ok(());
        };
        let desc = BufferDesc {
            width,
            height,
            ..self.settings.desc
        };
        Ok(self.producer.configure(desc)?)
    }

    fn render(
        &mut self,
        pattern: Pattern,
        flip: bool,
        frame: impl FnOnce(Frame) -> Frame,
    ) -> Result<(), TestSurfaceError> {
        self.configure()?;
        let cx = self.producer.cx().ok_or(SurfaceError::NotAttached)?.clone();
        if self.size().is_none() {
            return Err(TestSurfaceError::NoSize);
        }
        let (buffer, shared) = self
            .producer
            .acquire()
            .ok_or(TestSurfaceError::NoFreeBuffer)?;
        let texture = shared.texture();
        let painter = self
            .painter
            .get_or_insert_with(|| Painter::new(cx.device(), texture.format()));
        let uniform = PatternUniform {
            colors: pattern
                .colors()
                .map(|color| color.map(|channel| f32::from(channel) / 255.0)),
            flip: [f32::from(u8::from(flip)), 0.0, 0.0, 0.0],
        };
        cx.queue()
            .write_buffer(&painter.uniform, 0, bytes_of(&uniform));
        let view = texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = cx
            .device()
            .create_command_encoder(&CommandEncoderDescriptor::default());
        let mut pass = begin_pass(&mut encoder, &view, LoadOp::Clear(ClearColor::TRANSPARENT));
        pass.set_pipeline(&painter.pipeline);
        pass.set_bind_group(0, &painter.bind_group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        cx.queue().submit([encoder.finish()]);
        self.producer.submit(frame(Frame::new(buffer)));
        Ok(())
    }
}
