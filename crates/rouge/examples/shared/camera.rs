use rouge::core::testing::{TestError, TestUi};
use rouge::render::headless::EncodeError;
use std::fs::write;
use std::io::Error as IoError;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum CameraError {
    #[error("failed to write a snapshot: {0}")]
    Io(#[from] IoError),
    #[error(transparent)]
    Encode(#[from] EncodeError),
    #[error(transparent)]
    Test(#[from] TestError),
}

pub(crate) struct Camera<'a> {
    pub(crate) ui: TestUi,
    pub(crate) dir: &'a str,
}

impl Camera<'_> {
    pub(crate) fn save(&mut self, name: &str) -> Result<(), CameraError> {
        let path = format!("{}/{name}.png", self.dir);
        write(&path, self.ui.render()?.to_png()?)?;
        println!("wrote {path} ({:?})", self.ui.stats());
        Ok(())
    }

    pub(crate) fn shot(&mut self, name: &str) -> Result<(), CameraError> {
        self.ui.run(1.0);
        self.save(name)
    }
}
