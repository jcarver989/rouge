use crate::Gpu;
use crate::gpu::create_texture;
use wgpu::{
    CurrentSurfaceTexture, Surface, SurfaceConfiguration, SurfaceTexture, Texture, TextureFormat,
    TextureUsages, TextureView, TextureViewDescriptor,
};

pub struct TargetFrame {
    pub view: TextureView,
    texture: Option<SurfaceTexture>,
}

pub enum Acquire {
    Frame(TargetFrame),
    Retry,
    Skip,
}

pub enum Target {
    Offscreen(Texture),
    Swapchain {
        surface: Surface<'static>,
        config: SurfaceConfiguration,
        before_present: Option<Box<dyn Fn() + Send>>,
    },
}

impl Target {
    pub fn offscreen(gpu: &Gpu, width: u32, height: u32) -> Self {
        Target::Offscreen(create_texture(
            &gpu.device,
            "rouge offscreen target",
            (width.max(1), height.max(1)),
            TextureFormat::Rgba8Unorm,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        ))
    }

    pub fn swapchain(gpu: &Gpu, surface: Surface<'static>, config: SurfaceConfiguration) -> Self {
        surface.configure(&gpu.device, &config);
        Target::Swapchain {
            surface,
            config,
            before_present: None,
        }
    }

    pub fn before_present(mut self, notify: impl Fn() + Send + 'static) -> Self {
        if let Target::Swapchain { before_present, .. } = &mut self {
            *before_present = Some(Box::new(notify));
        }
        self
    }

    pub fn format(&self) -> TextureFormat {
        match self {
            Target::Offscreen(texture) => texture.format(),
            Target::Swapchain { config, .. } => config.format,
        }
    }

    pub fn size(&self) -> (u32, u32) {
        match self {
            Target::Offscreen(texture) => (texture.width(), texture.height()),
            Target::Swapchain { config, .. } => (config.width, config.height),
        }
    }

    pub fn texture(&self) -> Option<&Texture> {
        match self {
            Target::Offscreen(texture) => Some(texture),
            Target::Swapchain { .. } => None,
        }
    }

    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        if (width, height) == self.size() || width == 0 || height == 0 {
            return;
        }
        match self {
            Target::Offscreen(_) => *self = Target::offscreen(gpu, width, height),
            Target::Swapchain {
                surface, config, ..
            } => {
                (config.width, config.height) = (width, height);
                surface.configure(&gpu.device, config);
            }
        }
    }

    pub fn acquire(&mut self, gpu: &Gpu) -> Acquire {
        let (texture, surface) = match self {
            Target::Offscreen(texture) => (texture.clone(), None),
            Target::Swapchain {
                surface, config, ..
            } => match surface.get_current_texture() {
                CurrentSurfaceTexture::Success(frame)
                | CurrentSurfaceTexture::Suboptimal(frame) => (frame.texture.clone(), Some(frame)),
                CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                    surface.configure(&gpu.device, config);
                    return Acquire::Retry;
                }
                CurrentSurfaceTexture::Timeout => return Acquire::Retry,
                _ => return Acquire::Skip,
            },
        };
        Acquire::Frame(TargetFrame {
            view: texture.create_view(&TextureViewDescriptor::default()),
            texture: surface,
        })
    }

    pub fn present(&self, gpu: &Gpu, frame: TargetFrame) {
        if let Target::Swapchain {
            before_present: Some(notify),
            ..
        } = self
        {
            notify();
        }
        if let Some(texture) = frame.texture {
            gpu.queue.present(texture);
        }
    }
}
