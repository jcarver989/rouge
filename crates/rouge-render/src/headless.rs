use crate::{Atlas, Color, Gpu, GpuError, Item, RenderStats, Renderer, Viewport};
use image::ExtendedColorType::Rgba8;
use image::codecs::png::PngEncoder;
use image::{ImageEncoder, ImageError};
use pollster::block_on;
use thiserror::Error;
use wgpu::{
    BufferDescriptor, BufferUsages, COPY_BYTES_PER_ROW_ALIGNMENT, CommandEncoderDescriptor,
    Extent3d, Instance, InstanceDescriptor, InstanceFlags, MapMode, PollType, TexelCopyBufferInfo,
    TexelCopyBufferLayout, Texture, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureView, TextureViewDescriptor,
};

#[derive(Debug, Error)]
#[error("failed to encode a PNG: {0}")]
pub struct EncodeError(#[from] ImageError);

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(x < self.width && y < self.height, "pixel out of bounds");
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    pub fn to_png(&self) -> Result<Vec<u8>, EncodeError> {
        let mut png = Vec::new();
        PngEncoder::new(&mut png).write_image(&self.pixels, self.width, self.height, Rgba8)?;
        Ok(png)
    }
}

pub fn assert_near(actual: [u8; 4], expected: [u8; 4], what: &str) {
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (i32::from(*a) - i32::from(e)).abs() <= 2,
            "{what}: expected {expected:?}, got {actual:?}"
        );
    }
}

pub struct Headless {
    gpu: Gpu,
    renderer: Renderer,
    texture: Texture,
    view: TextureView,
    viewport: Viewport,
}

impl Headless {
    pub fn new(viewport: Viewport) -> Result<Self, GpuError> {
        let mut descriptor = InstanceDescriptor::new_without_display_handle();
        descriptor.flags = InstanceFlags::from_build_config().difference(InstanceFlags::DEBUG);
        let instance = Instance::new(descriptor);
        let gpu = block_on(Gpu::new(instance, None))?;
        let format = TextureFormat::Rgba8Unorm;
        let texture = gpu.device.create_texture(&TextureDescriptor {
            label: Some("rouge headless target"),
            size: Extent3d {
                width: viewport.width,
                height: viewport.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let renderer = Renderer::new(&gpu.device, &gpu.queue, format);
        Ok(Headless {
            gpu,
            renderer,
            texture,
            view,
            viewport,
        })
    }

    pub fn render(&mut self, clear: Color, items: &[Item<'_>], atlas: &mut Atlas) -> RenderStats {
        self.renderer
            .render(&self.view, self.viewport, clear, items, atlas);
        self.renderer.stats()
    }

    pub fn wait(&self) -> Result<(), GpuError> {
        self.gpu.device.poll(PollType::wait_indefinitely())?;
        Ok(())
    }

    pub fn image(&self) -> Result<Image, GpuError> {
        let Viewport { width, height, .. } = self.viewport;
        let unpadded = width * 4;
        let padded = unpadded.next_multiple_of(COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.gpu.device.create_buffer(&BufferDescriptor {
            label: Some("rouge headless readback"),
            size: u64::from(padded * height),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            self.texture.size(),
        );
        self.gpu.queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        slice.map_async(MapMode::Read, |_| {});
        self.wait()?;
        let data = slice.get_mapped_range()?;
        let pixels = data
            .chunks(padded as usize)
            .flat_map(|row| &row[..unpadded as usize])
            .copied()
            .collect();
        Ok(Image {
            width,
            height,
            pixels,
        })
    }
}
