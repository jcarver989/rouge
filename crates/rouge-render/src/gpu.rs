use thiserror::Error;
use wgpu::{
    Adapter, BlendState, Color as ClearColor, ColorTargetState, ColorWrites, CommandEncoder,
    Device, DeviceDescriptor, Extent3d, FragmentState, Instance, LoadOp, MultisampleState,
    Operations, PipelineCompilationOptions, PipelineLayout, PowerPreference, PrimitiveState,
    PrimitiveTopology, Queue, RenderPass, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, RequestAdapterError, RequestAdapterOptions,
    RequestDeviceError, ShaderModule, StoreOp, Surface, Texture, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages, TextureView, VertexBufferLayout, VertexState,
};

#[cfg(not(target_arch = "wasm32"))]
use wgpu::{MapRangeError, PollError};

#[derive(Clone)]
pub struct Gpu {
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
}

#[derive(Debug, Error)]
pub enum GpuError {
    #[error("no suitable GPU adapter found: {0}")]
    NoAdapter(#[from] RequestAdapterError),
    #[error("failed to create GPU device: {0}")]
    NoDevice(#[from] RequestDeviceError),
    #[cfg(not(target_arch = "wasm32"))]
    #[error("failed to wait for the GPU: {0}")]
    Poll(#[from] PollError),
    #[cfg(not(target_arch = "wasm32"))]
    #[error("failed to read back the rendered frame: {0}")]
    Readback(#[from] MapRangeError),
}

impl Gpu {
    pub async fn new(
        instance: Instance,
        compatible_surface: Option<&Surface<'_>>,
    ) -> Result<Gpu, GpuError> {
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                compatible_surface,
                power_preference: PowerPreference::from_env().unwrap_or_default(),
                ..Default::default()
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("rouge device"),
                ..Default::default()
            })
            .await?;
        Ok(Gpu {
            adapter,
            device,
            queue,
        })
    }
}

pub(crate) fn create_texture(
    device: &Device,
    label: &str,
    (width, height): (u32, u32),
    format: TextureFormat,
    usage: TextureUsages,
) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

pub(crate) fn begin_pass(
    encoder: &mut CommandEncoder,
    view: &TextureView,
    load: LoadOp<ClearColor>,
) -> RenderPass<'static> {
    encoder
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("rouge pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
        .forget_lifetime()
}

pub(crate) struct PipelineDesc<'a> {
    pub layout: &'a PipelineLayout,
    pub module: &'a ShaderModule,
    pub fragment: &'a str,
    pub format: TextureFormat,
    pub blend: Option<BlendState>,
    pub buffers: &'a [Option<VertexBufferLayout<'a>>],
}

pub(crate) fn create_pipeline(device: &Device, desc: &PipelineDesc<'_>) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(desc.fragment),
        layout: Some(desc.layout),
        vertex: VertexState {
            module: desc.module,
            entry_point: Some("vs_main"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: desc.buffers,
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: desc.module,
            entry_point: Some(desc.fragment),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: desc.format,
                blend: desc.blend,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
