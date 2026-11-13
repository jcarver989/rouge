use thiserror::Error;
use wgpu::{
    Adapter, Device, DeviceDescriptor, Instance, PowerPreference, Queue, RequestAdapterError,
    RequestAdapterOptions, RequestDeviceError, Surface,
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
