use crate::atlas::AtlasRect;
use crate::image::{ImageData, ImageFormat};
use crate::renderer::upload;
use crate::{Color, Corners, Edges, Point, Rect, Size};
use bytemuck::{Pod, Zeroable, cast_slice};
use std::cell::{Cell, Ref, RefCell};
use std::ops::Range;
use wgpu::{Buffer, Device, Queue};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub bounds: Rect,
    pub clip: Rect,
    pub background: Color,
    pub border_color: Color,
    pub corner_radii: Corners,
    pub border_widths: Edges,
    pub backing: bool,
}

impl Quad {
    pub const fn new(bounds: Rect, background: Color) -> Self {
        Quad {
            bounds,
            clip: Rect::EVERYTHING,
            background,
            border_color: Color::TRANSPARENT,
            corner_radii: Corners::ZERO,
            border_widths: Edges::ZERO,
            backing: false,
        }
    }

    pub const fn backing(bounds: Rect) -> Self {
        Quad {
            backing: true,
            ..Quad::new(bounds, Color::TRANSPARENT)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub bounds: Rect,
    pub clip: Rect,
    pub corner_radii: Corners,
    pub color: Color,
    pub blur_radius: f32,
    pub backing: bool,
}

impl Shadow {
    pub const fn new(bounds: Rect, color: Color, blur_radius: f32) -> Self {
        Shadow {
            bounds,
            clip: Rect::EVERYTHING,
            corner_radii: Corners::ZERO,
            color,
            blur_radius,
            backing: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub origin: Point,
    pub clip: Rect,
    pub color: Color,
    pub mask: AtlasRect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageQuad {
    pub bounds: Rect,
    pub clip: Rect,
    pub corner_radii: Corners,
    pub source: Rect,
    pub tint: Color,
    pub image: ImageData,
}

impl ImageQuad {
    pub const fn new(bounds: Rect, image: ImageData) -> Self {
        ImageQuad {
            bounds,
            clip: Rect::EVERYTHING,
            corner_radii: Corners::ZERO,
            source: Rect::new(0.0, 0.0, 1.0, 1.0),
            tint: Color::WHITE,
            image,
        }
    }
}

pub(crate) struct Batch {
    pub image: Option<ImageData>,
    pub instances: Range<u32>,
}

const QUAD: f32 = 0.0;
const SHADOW: f32 = 1.0;
const GLYPH: f32 = 2.0;
const IMAGE: f32 = 3.0;

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub(crate) struct Instance {
    bounds: [f32; 4],
    clip: [f32; 4],
    color: [f32; 4],
    border_color: [f32; 4],
    radii: [f32; 4],
    params: [f32; 4],
    kind: [f32; 4],
}

#[derive(Default)]
pub struct DrawList {
    instances: Vec<Instance>,
    batches: Vec<Batch>,
    extent: Option<Rect>,
    revision: u64,
    buffer: RefCell<Option<Buffer>>,
    uploaded: Cell<bool>,
}

impl DrawList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.instances.clear();
        self.batches.clear();
        self.extent = None;
        self.changed();
    }

    pub fn replace(&mut self, other: DrawList) {
        self.instances = other.instances;
        self.batches = other.batches;
        self.extent = other.extent;
        self.changed();
    }

    pub fn extent(&self) -> Option<Rect> {
        self.extent
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn changed(&mut self) {
        self.revision += 1;
        self.uploaded.set(false);
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn push_quad(&mut self, quad: Quad) {
        if quad.bounds.is_empty() || quad.clip.intersect(&quad.bounds).is_empty() {
            return;
        }
        let backing = f32::from(u8::from(quad.backing));
        self.push(
            None,
            quad.bounds.inflate(1.0).intersect(&quad.clip),
            Instance {
                bounds: quad.bounds.to_array(),
                clip: quad.clip.to_array(),
                color: quad.background.to_array(),
                border_color: quad.border_color.to_array(),
                radii: quad.corner_radii.to_array(),
                params: quad.border_widths.to_array(),
                kind: [QUAD, backing, backing, 0.0],
            },
        );
    }

    pub fn push_shadow(&mut self, shadow: Shadow) {
        if shadow.color.a <= 0.0 {
            return;
        }
        let reach = shadow.blur_radius.max(0.0) * 1.5 + 2.0;
        self.push(
            None,
            shadow.bounds.inflate(reach).intersect(&shadow.clip),
            Instance {
                bounds: shadow.bounds.to_array(),
                clip: shadow.clip.to_array(),
                color: shadow.color.to_array(),
                radii: shadow.corner_radii.to_array(),
                params: [shadow.blur_radius.max(0.0) * 0.5, 0.0, 0.0, 0.0],
                kind: [SHADOW, 0.0, f32::from(u8::from(shadow.backing)), 0.0],
                ..Default::default()
            },
        );
    }

    pub fn push_glyph(&mut self, glyph: Glyph) {
        if glyph.color.a <= 0.0 {
            return;
        }
        let mask = glyph.mask;
        let reach = Rect {
            origin: glyph.origin,
            size: Size::new(mask.width as f32, mask.height as f32),
        };
        self.push(
            None,
            reach.inflate(1.0).intersect(&glyph.clip),
            Instance {
                bounds: [glyph.origin.x, glyph.origin.y, 0.0, 0.0],
                clip: glyph.clip.to_array(),
                color: glyph.color.to_array(),
                params: [
                    mask.x as f32,
                    mask.y as f32,
                    mask.width as f32,
                    mask.height as f32,
                ],
                kind: [GLYPH, 0.0, 0.0, 0.0],
                ..Default::default()
            },
        );
    }

    pub fn push_image(&mut self, image: ImageQuad) {
        let ImageQuad {
            bounds,
            clip,
            corner_radii,
            source,
            tint,
            image,
        } = image;
        if bounds.is_empty() || clip.intersect(&bounds).is_empty() || tint.a <= 0.0 {
            return;
        }
        let mask = f32::from(u8::from(image.format() == ImageFormat::Alpha8));
        self.push(
            Some(image),
            bounds.inflate(1.0).intersect(&clip),
            Instance {
                bounds: bounds.to_array(),
                clip: clip.to_array(),
                color: tint.to_array(),
                radii: corner_radii.to_array(),
                params: source.to_array(),
                kind: [IMAGE, 0.0, 0.0, mask],
                ..Default::default()
            },
        );
    }

    fn push(&mut self, image: Option<ImageData>, reach: Rect, instance: Instance) {
        let index = self.instances.len() as u32;
        match self.batches.last_mut().filter(|batch| batch.image == image) {
            Some(batch) => batch.instances.end = index + 1,
            None => self.batches.push(Batch {
                image,
                instances: index..index + 1,
            }),
        }
        self.instances.push(instance);
        self.extent = Some(self.extent.map_or(reach, |extent| extent.union(&reach)));
        self.changed();
    }

    pub(crate) fn sync(&self, device: &Device, queue: &Queue) -> u64 {
        if self.uploaded.replace(true) {
            return 0;
        }
        upload(
            &mut self.buffer.borrow_mut(),
            device,
            queue,
            cast_slice(&self.instances),
        )
    }

    pub(crate) fn batches(&self) -> &[Batch] {
        &self.batches
    }

    pub(crate) fn buffer(&self) -> Ref<'_, Option<Buffer>> {
        self.buffer.borrow()
    }
}
