use thiserror::Error;
use wgpu::{
    Adapter, Device, DeviceDescriptor, Instance, MapRangeError, PollError, Queue,
    RequestAdapterError, RequestAdapterOptions, RequestDeviceError, Surface,
};

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
    #[error("failed to wait for the GPU: {0}")]
    Poll(#[from] PollError),
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
