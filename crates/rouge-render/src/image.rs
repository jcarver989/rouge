use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

static NEXT_IMAGE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Rgba8,
    Alpha8,
}

impl ImageFormat {
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            ImageFormat::Rgba8 => 4,
            ImageFormat::Alpha8 => 1,
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ImageError {
    #[error(
        "image is {width}x{height} but {actual} bytes were given where {expected} were expected"
    )]
    SizeMismatch {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    #[error("image has no pixels")]
    Empty,
}

#[derive(Clone)]
pub struct ImageData {
    id: u64,
    width: u32,
    height: u32,
    format: ImageFormat,
    pixels: Arc<[u8]>,
}

impl ImageData {
    fn new(
        width: u32,
        height: u32,
        format: ImageFormat,
        pixels: Vec<u8>,
    ) -> Result<Self, ImageError> {
        let expected = width as usize * height as usize * format.bytes_per_pixel() as usize;
        if expected == 0 {
            return Err(ImageError::Empty);
        }
        if pixels.len() != expected {
            return Err(ImageError::SizeMismatch {
                width,
                height,
                expected,
                actual: pixels.len(),
            });
        }
        let id = NEXT_IMAGE.fetch_add(1, Ordering::Relaxed);
        Ok(ImageData {
            id,
            width,
            height,
            format,
            pixels: pixels.into(),
        })
    }

    pub fn rgba8(width: u32, height: u32, mut pixels: Vec<u8>) -> Result<Self, ImageError> {
        for pixel in pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .filter(|pixel| pixel[3] != 255)
        {
            let alpha = u32::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
        Self::new(width, height, ImageFormat::Rgba8, pixels)
    }

    pub fn premultiplied_rgba8(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> Result<Self, ImageError> {
        Self::new(width, height, ImageFormat::Rgba8, pixels)
    }

    pub fn alpha8(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, ImageError> {
        Self::new(width, height, ImageFormat::Alpha8, pixels)
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn format(&self) -> ImageFormat {
        self.format
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn byte_len(&self) -> u64 {
        self.pixels.len() as u64
    }
}

impl PartialEq for ImageData {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ImageData {}

impl Debug for ImageData {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "ImageData({}, {}x{}, {:?})",
            self.id, self.width, self.height, self.format
        )
    }
}
