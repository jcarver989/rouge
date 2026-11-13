use crate::InteropError;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString};
use objc2_io_surface::{
    IOSurfaceLockOptions, IOSurfaceRef, kIOSurfaceBytesPerElement, kIOSurfaceHeight,
    kIOSurfacePixelFormat, kIOSurfaceWidth,
};
use objc2_metal::{
    MTLDevice, MTLPixelFormat, MTLStorageMode, MTLTexture, MTLTextureDescriptor, MTLTextureType,
    MTLTextureUsage,
};
use std::ptr::null_mut;
use std::slice::from_raw_parts_mut;
use wgpu::hal::api::Metal;
use wgpu::hal::{CopyExtent, metal};
use wgpu::{
    Device, Extent3d, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    TextureUses,
};

const BGRA: i32 = 0x4247_5241;
const BYTES_PER_PIXEL: i32 = 4;

#[derive(Clone, Debug)]
pub struct IoSurface(CFRetained<IOSurfaceRef>);

unsafe impl Send for IoSurface {}
unsafe impl Sync for IoSurface {}

impl IoSurface {
    pub fn new(width: u32, height: u32) -> Result<IoSurface, InteropError> {
        let failed = InteropError::Allocation { width, height };
        let (Ok(w), Ok(h)) = (i32::try_from(width), i32::try_from(height)) else {
            return Err(failed);
        };
        if w == 0 || h == 0 {
            return Err(failed);
        }
        let keys: [&CFString; 4] = unsafe {
            [
                kIOSurfaceWidth,
                kIOSurfaceHeight,
                kIOSurfaceBytesPerElement,
                kIOSurfacePixelFormat,
            ]
        };
        let numbers = [w, h, BYTES_PER_PIXEL, BGRA].map(CFNumber::new_i32);
        let values: Vec<&CFNumber> = numbers.iter().map(|number| &**number).collect();
        let properties = CFDictionary::from_slices(&keys, &values);
        let surface = unsafe { IOSurfaceRef::new(properties.as_opaque()) }.ok_or(failed)?;
        Ok(IoSurface(surface))
    }

    pub fn width(&self) -> u32 {
        self.0.width() as u32
    }

    pub fn height(&self) -> u32 {
        self.0.height() as u32
    }

    pub fn global_id(&self) -> u32 {
        self.0.id()
    }

    pub fn as_raw(&self) -> &IOSurfaceRef {
        &self.0
    }

    pub fn write<T>(&self, f: impl FnOnce(&mut [u8], usize) -> T) -> Result<T, InteropError> {
        let options = IOSurfaceLockOptions::empty();
        if unsafe { self.0.lock(options, null_mut()) } != 0 {
            return Err(InteropError::Import("the IOSurface could not be locked"));
        }
        let stride = self.0.bytes_per_row();
        let len = stride * self.0.height();
        let bytes = unsafe { from_raw_parts_mut(self.0.base_address().as_ptr().cast(), len) };
        let result = f(bytes, stride);
        unsafe { self.0.unlock(options, null_mut()) };
        Ok(result)
    }
}

pub fn import_io_surface(
    device: &Device,
    surface: &IoSurface,
    format: TextureFormat,
    usage: TextureUsages,
) -> Result<Texture, InteropError> {
    if format != TextureFormat::Bgra8Unorm {
        return Err(InteropError::Format {
            format,
            what: "a BGRA IOSurface",
        });
    }
    let (width, height) = (surface.width(), surface.height());
    let hal = unsafe { device.as_hal::<Metal>() }.ok_or(InteropError::Unsupported(
        "IOSurfaces outside the Metal backend",
    ))?;
    let raw = create_metal_texture(&hal, surface, usage)?;
    let extent = CopyExtent {
        width,
        height,
        depth: 1,
    };
    let texture = unsafe {
        metal::Device::texture_from_raw(raw, format, MTLTextureType::Type2D, 1, 1, extent, None)
    };
    drop(hal);
    let descriptor = TextureDescriptor {
        label: Some("rouge io surface"),
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
    };
    Ok(unsafe {
        device.create_texture_from_hal::<Metal>(texture, &descriptor, TextureUses::UNINITIALIZED)
    })
}

fn create_metal_texture(
    hal: &metal::Device,
    surface: &IoSurface,
    usage: TextureUsages,
) -> Result<Retained<ProtocolObject<dyn MTLTexture>>, InteropError> {
    let descriptor = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            MTLPixelFormat::BGRA8Unorm,
            surface.width() as usize,
            surface.height() as usize,
            false,
        )
    };
    let mut metal_usage = MTLTextureUsage::ShaderRead;
    if usage.contains(TextureUsages::RENDER_ATTACHMENT) {
        metal_usage |= MTLTextureUsage::RenderTarget;
    }
    if usage.contains(TextureUsages::STORAGE_BINDING) {
        metal_usage |= MTLTextureUsage::ShaderWrite;
    }
    descriptor.setUsage(metal_usage);
    descriptor.setStorageMode(MTLStorageMode::Shared);
    hal.raw_device()
        .newTextureWithDescriptor_iosurface_plane(&descriptor, surface.as_raw(), 0)
        .ok_or(InteropError::Import(
            "Metal refused to create a texture over the IOSurface",
        ))
}
