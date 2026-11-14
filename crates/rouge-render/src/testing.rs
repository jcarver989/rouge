use crate::gpu::{PipelineDesc, begin_pass, create_pipeline};
use crate::surface::{
    BufferDesc, BufferRef, BufferSetId, Fence, FenceSignal, Frame, FrameId, QueueMode,
    SharedBuffer, Sharing, SurfaceConfig, SurfaceCx, SurfaceError, SurfaceEvent, SurfaceId,
    SurfaceProducer, SurfaceSource, surface_channel,
};
use bytemuck::{Pod, Zeroable, bytes_of};
use std::collections::VecDeque;
use thiserror::Error;
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingType, BufferBindingType, BufferUsages, Color as ClearColor, CommandEncoderDescriptor,
    LoadOp, PipelineLayoutDescriptor, RenderPipeline, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, TextureFormat, TextureViewDescriptor,
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
    #[error("the surface has not been attached to a compositor")]
    NotAttached,
    #[error("the surface has no size yet")]
    NoSize,
    #[error("every buffer is still held by the compositor")]
    NoFreeBuffer,
    #[error(transparent)]
    Surface(#[from] SurfaceError),
}

#[derive(Clone, Copy, Debug)]
pub struct TestSurfaceBuilder {
    queue: QueueMode,
    desc: BufferDesc,
    size: Option<(u32, u32)>,
}

impl Default for TestSurfaceBuilder {
    fn default() -> Self {
        TestSurfaceBuilder {
            queue: QueueMode::default(),
            desc: BufferDesc::new(0, 0),
            size: None,
        }
    }
}

impl TestSurfaceBuilder {
    pub fn queue(mut self, queue: QueueMode) -> Self {
        self.queue = queue;
        self
    }

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
        let (producer, source) = surface_channel(self.queue);
        let surface = TestSurface {
            producer,
            settings: self,
            cx: None,
            config: None,
            chain: None,
            free: VecDeque::new(),
            pipeline: None,
            detached: false,
            log: Vec::new(),
        };
        (surface, source)
    }
}

struct Chain {
    set: BufferSetId,
    buffers: Vec<SharedBuffer>,
    size: (u32, u32),
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PatternUniform {
    colors: [[f32; 4]; 4],
    flip: [f32; 4],
}

pub struct TestSurface {
    producer: SurfaceProducer,
    settings: TestSurfaceBuilder,
    cx: Option<SurfaceCx>,
    config: Option<SurfaceConfig>,
    chain: Option<Chain>,
    free: VecDeque<u32>,
    pipeline: Option<RenderPipeline>,
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
            match &event {
                SurfaceEvent::Attached(cx) => self.cx = Some((**cx).clone()),
                SurfaceEvent::Configure(config) => self.config = Some(*config),
                SurfaceEvent::Released(buffer) => {
                    if self.chain.as_ref().is_some_and(|c| c.set == buffer.set)
                        && !self.free.contains(&buffer.index)
                    {
                        self.free.push_back(buffer.index);
                    }
                }
                SurfaceEvent::Detached => self.detached = true,
                SurfaceEvent::FrameRequest { .. }
                | SurfaceEvent::Presented { .. }
                | SurfaceEvent::Discarded(_) => {}
            }
            self.log.push(event);
        }
        self.allocate()?;
        Ok(&self.log)
    }

    pub fn present(&mut self, pattern: Pattern) -> Result<FrameId, TestSurfaceError> {
        self.present_frame(pattern, |frame| frame)
    }

    pub fn present_frame(
        &mut self,
        pattern: Pattern,
        frame: impl FnOnce(Frame) -> Frame,
    ) -> Result<FrameId, TestSurfaceError> {
        self.render(pattern, false, frame)
    }

    pub fn present_flipped(
        &mut self,
        pattern: Pattern,
        frame: impl FnOnce(Frame) -> Frame,
    ) -> Result<FrameId, TestSurfaceError> {
        self.render(pattern, true, frame)
    }

    pub fn fence(&self) -> (FenceSignal, Fence) {
        self.producer.fence()
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
        self.cx.as_ref()
    }

    pub fn free_buffers(&self) -> usize {
        self.free.len()
    }

    pub fn buffers(&self) -> &[SharedBuffer] {
        self.chain.as_ref().map_or(&[], |chain| &chain.buffers)
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

    fn allocate(&mut self) -> Result<(), TestSurfaceError> {
        let (Some(cx), Some(size)) = (self.cx.clone(), self.size()) else {
            return Ok(());
        };
        if self.chain.as_ref().is_some_and(|chain| chain.size == size) {
            return Ok(());
        }
        let desc = BufferDesc {
            width: size.0,
            height: size.1,
            ..self.settings.desc
        };
        let buffers = cx.allocate(&desc)?;
        let set = self.producer.register(&buffers);
        if let Some(old) = self.chain.take() {
            self.producer.unregister(old.set);
        }
        self.free = (0..buffers.len() as u32).collect();
        self.chain = Some(Chain { set, buffers, size });
        Ok(())
    }

    fn render(
        &mut self,
        pattern: Pattern,
        flip: bool,
        frame: impl FnOnce(Frame) -> Frame,
    ) -> Result<FrameId, TestSurfaceError> {
        self.allocate()?;
        let cx = self.cx.clone().ok_or(TestSurfaceError::NotAttached)?;
        let chain = self.chain.as_ref().ok_or(TestSurfaceError::NoSize)?;
        let index = self
            .free
            .pop_front()
            .ok_or(TestSurfaceError::NoFreeBuffer)?;
        let set = chain.set;
        let texture = chain.buffers[index as usize].texture().clone();
        let format = texture.format();
        if self.pipeline.is_none() {
            self.pipeline = Some(pattern_pipeline(&cx, format));
        }
        let pipeline = self
            .pipeline
            .as_ref()
            .expect("the pattern pipeline was created");
        let device = cx.device();
        let uniform = PatternUniform {
            colors: pattern
                .colors()
                .map(|color| color.map(|channel| f32::from(channel) / 255.0)),
            flip: [f32::from(u8::from(flip)), 0.0, 0.0, 0.0],
        };
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("rouge test pattern"),
            contents: bytes_of(&uniform),
            usage: BufferUsages::UNIFORM,
        });
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("rouge test pattern"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        let mut pass = begin_pass(&mut encoder, &view, LoadOp::Clear(ClearColor::TRANSPARENT));
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        cx.queue().submit([encoder.finish()]);
        Ok(self
            .producer
            .submit(frame(Frame::new(BufferRef { set, index }))))
    }
}

fn pattern_pipeline(cx: &SurfaceCx, format: TextureFormat) -> RenderPipeline {
    let device = cx.device();
    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("rouge test pattern"),
        source: ShaderSource::Wgsl(PATTERN_WGSL.into()),
    });
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("rouge test pattern"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("rouge test pattern"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    create_pipeline(
        device,
        &PipelineDesc {
            layout: &pipeline_layout,
            module: &module,
            fragment: "fs_main",
            format,
            blend: None,
            buffers: &[],
        },
    )
}
