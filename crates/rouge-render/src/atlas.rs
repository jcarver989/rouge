#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

struct Shelf {
    y: u32,
    height: u32,
    cursor: u32,
}

pub struct Atlas {
    height: u32,
    pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    dirty_rows: Option<(u32, u32)>,
}

const INITIAL_HEIGHT: u32 = 512;
const PADDING: u32 = 1;

impl Default for Atlas {
    fn default() -> Self {
        Self::new()
    }
}

impl Atlas {
    pub const MAX_HEIGHT: u32 = 8192;
    pub const WIDTH: u32 = 2048;

    pub fn new() -> Self {
        Atlas {
            height: INITIAL_HEIGHT,
            pixels: vec![0; (Self::WIDTH * INITIAL_HEIGHT) as usize],
            shelves: Vec::new(),
            dirty_rows: None,
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (Self::WIDTH, self.height)
    }

    pub(crate) fn rows(&self, start: u32, end: u32) -> &[u8] {
        &self.pixels[(start * Self::WIDTH) as usize..(end * Self::WIDTH) as usize]
    }

    pub(crate) fn take_dirty_rows(&mut self) -> Option<(u32, u32)> {
        self.dirty_rows.take()
    }

    pub fn insert(&mut self, width: u32, height: u32, data: &[u8]) -> Option<AtlasRect> {
        assert_eq!(data.len(), (width * height) as usize, "mask size mismatch");
        if width == 0 || height == 0 || width + PADDING > Self::WIDTH {
            return None;
        }
        let (x, y) = self.allocate(width, height)?;
        for (row, line) in data.chunks(width as usize).enumerate() {
            let dst = ((y + row as u32) * Self::WIDTH + x) as usize;
            self.pixels[dst..dst + width as usize].copy_from_slice(line);
        }
        let (lo, hi) = self.dirty_rows.unwrap_or((y, y + height));
        self.dirty_rows = Some((lo.min(y), hi.max(y + height)));
        Some(AtlasRect {
            x,
            y,
            width,
            height,
        })
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let best = self
            .shelves
            .iter_mut()
            .filter(|s| {
                s.height >= height
                    && s.height <= height + height / 4 + 2
                    && s.cursor + width <= Self::WIDTH
            })
            .min_by_key(|s| s.height);
        if let Some(shelf) = best {
            let x = shelf.cursor;
            shelf.cursor += width + PADDING;
            return Some((x, shelf.y));
        }
        let y = self.shelves.last().map_or(0, |s| s.y + s.height + PADDING);
        while y + height > self.height {
            if self.height >= Self::MAX_HEIGHT {
                return None;
            }
            self.height = (self.height * 2).min(Self::MAX_HEIGHT);
            self.pixels.resize((Self::WIDTH * self.height) as usize, 0);
        }
        self.shelves.push(Shelf {
            y,
            height,
            cursor: width + PADDING,
        });
        Some((0, y))
    }
}
