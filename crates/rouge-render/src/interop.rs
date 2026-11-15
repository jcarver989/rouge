#![allow(unsafe_code)]

#[cfg(target_os = "macos")]
mod io_surface;

#[cfg(target_os = "macos")]
pub use io_surface::{IoSurface, import_io_surface};
