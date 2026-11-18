#[path = "shared/gallery/mod.rs"]
mod gallery;

use rouge::{App, AppError, WindowOptions};
#[cfg(not(target_arch = "wasm32"))]
use std::env::args;
use thiserror::Error;

#[derive(Debug, Error)]
enum GalleryError {
    #[error(transparent)]
    App(#[from] AppError),
    #[cfg(not(target_arch = "wasm32"))]
    #[error(transparent)]
    Snapshot(#[from] gallery::CameraError),
}

fn main() -> Result<(), GalleryError> {
    #[cfg(not(target_arch = "wasm32"))]
    if args().nth(1).as_deref() == Some("--snapshot") {
        return Ok(gallery::snapshot(
            &args().nth(2).unwrap_or_else(|| ".".into()),
        )?);
    }
    let options = WindowOptions::default()
        .title("Rouge component gallery")
        .size(1296.0, 800.0);
    Ok(App::new().window(options, gallery::gallery).run()?)
}
