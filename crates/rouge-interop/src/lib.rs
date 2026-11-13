#![allow(unsafe_code)]

#[cfg(target_os = "macos")]
mod io_surface;

#[cfg(target_os = "macos")]
pub use io_surface::{IoSurface, import_io_surface};

use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum InteropError {
    #[error("the GPU backend cannot import {0}")]
    Unsupported(&'static str),
    #[error("failed to allocate a {width}x{height} shared buffer")]
    Allocation { width: u32, height: u32 },
    #[error("failed to import a shared buffer: {0}")]
    Import(&'static str),
    #[error("{format:?} cannot be shared as {what}")]
    Format {
        format: wgpu::TextureFormat,
        what: &'static str,
    },
}
