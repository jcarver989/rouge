#![cfg(target_os = "macos")]

use pollster::block_on;
use rouge_interop::{InteropError, IoSurface, import_io_surface};
use std::error::Error;
use wgpu::{
    Device, DeviceDescriptor, Instance, InstanceDescriptor, RequestAdapterOptions, TextureFormat,
    TextureUsages,
};

#[test]
fn an_io_surface_has_its_size_and_keeps_what_is_written() -> Result<(), Box<dyn Error>> {
    let surface = IoSurface::new(6, 3)?;
    assert_eq!((surface.width(), surface.height()), (6, 3));
    assert_ne!(surface.global_id(), 0);
    surface.write(|bytes, stride| bytes[stride + 4..stride + 8].copy_from_slice(&[1, 2, 3, 4]))?;
    let read = surface.write(|bytes, stride| bytes[stride + 4..stride + 8].to_vec())?;
    assert_eq!(read, [1, 2, 3, 4]);
    Ok(())
}

#[test]
fn empty_io_surfaces_are_refused() {
    assert_eq!(
        IoSurface::new(0, 4).err(),
        Some(InteropError::Allocation {
            width: 0,
            height: 4
        })
    );
}

#[test]
fn an_io_surface_imports_as_a_texture_of_the_same_size() -> Result<(), Box<dyn Error>> {
    let device = device()?;
    let surface = IoSurface::new(16, 8)?;
    let texture = import_io_surface(
        &device,
        &surface,
        TextureFormat::Bgra8Unorm,
        TextureUsages::TEXTURE_BINDING,
    )?;
    assert_eq!((texture.width(), texture.height()), (16, 8));
    assert_eq!(texture.format(), TextureFormat::Bgra8Unorm);
    Ok(())
}

#[test]
fn only_bgra_io_surfaces_can_be_imported() -> Result<(), Box<dyn Error>> {
    let device = device()?;
    let surface = IoSurface::new(4, 4)?;
    let imported = import_io_surface(
        &device,
        &surface,
        TextureFormat::Rgba8Unorm,
        TextureUsages::TEXTURE_BINDING,
    );
    assert!(matches!(
        imported,
        Err(InteropError::Format {
            format: TextureFormat::Rgba8Unorm,
            ..
        })
    ));
    Ok(())
}

fn device() -> Result<Device, Box<dyn Error>> {
    let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
    let adapter = block_on(instance.request_adapter(&RequestAdapterOptions::default()))?;
    let (device, _queue) = block_on(adapter.request_device(&DeviceDescriptor::default()))?;
    Ok(device)
}
