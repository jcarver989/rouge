use crate::atlas::AtlasRect;
use crate::image::{ImageData, ImageFormat};
use crate::{Color, Corners, Edges, Point, Rect, Size};
use bytemuck::{Pod, Zeroable};
use slotmap::{SlotMap, new_key_type};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use wgpu::{VertexAttribute, vertex_attr_array};

static NEXT_LIST: AtomicU64 = AtomicU64::new(1);

new_key_type! {
    pub struct ListKey;
}

#[derive(Default)]
pub struct DrawLists(SlotMap<ListKey, DrawList>);

impl DrawLists {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, list: DrawList) -> ListKey {
        self.0.insert(list)
    }

    pub fn get(&self, key: ListKey) -> Option<&DrawList> {
        self.0.get(key)
    }

    pub fn get_mut(&mut self, key: ListKey) -> Option<&mut DrawList> {
        self.0.get_mut(key)
    }

    pub fn remove(&mut self, key: ListKey) -> Option<DrawList> {
        self.0.remove(key)
    }
}

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

    pub fn line(bounds: Rect, color: Color) -> Self {
        let Size { width, height } = bounds.size;
        let border_widths = if height <= width {
            Edges {
                top: height,
                ..Edges::ZERO
            }
        } else {
            Edges {
                left: width,
                ..Edges::ZERO
            }
        };
        Quad {
            border_color: color,
            border_widths,
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

const SHADOW: u32 = 1;
const GLYPH: u32 = 2;
const IMAGE: u32 = 3;
const LAYER_FILL: u32 = 4;
const LAYER_RADII: u32 = 8;
const ALPHA_MASK: u32 = 16;

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub(crate) struct Instance {
    bounds: [f32; 4],
    clip: [f32; 4],
    radii: [f32; 4],
    params: [f32; 4],
    color: [u16; 4],
    border_color: [u16; 4],
    flags: u32,
}

impl Instance {
    pub(crate) const ATTRIBUTES: [VertexAttribute; 7] = vertex_attr_array![
        0 => Float32x4,
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4,
        4 => Unorm16x4,
        5 => Unorm16x4,
        6 => Uint32,
    ];
    pub(crate) const SIZE: u64 = size_of::<Instance>() as u64;
}

fn unorm(color: Color) -> [u16; 4] {
    color
        .to_array()
        .map(|channel| (channel.clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as u16)
}

pub struct DrawList {
    id: u64,
    instances: Vec<Instance>,
    batches: Vec<Batch>,
    extent: Option<Rect>,
    revision: u64,
}

impl Default for DrawList {
    fn default() -> Self {
        DrawList {
            id: NEXT_LIST.fetch_add(1, Ordering::Relaxed),
            instances: Vec::new(),
            batches: Vec::new(),
            extent: None,
            revision: 0,
        }
    }
}

impl DrawList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.instances.clear();
        self.batches.clear();
        self.extent = None;
        self.revision += 1;
    }

    pub fn replace(&mut self, other: DrawList) {
        self.instances = other.instances;
        self.batches = other.batches;
        self.extent = other.extent;
        self.revision += 1;
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub fn extent(&self) -> Option<Rect> {
        self.extent
    }

    pub fn revision(&self) -> u64 {
        self.revision
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
        let backing = if quad.backing {
            LAYER_FILL | LAYER_RADII
        } else {
            0
        };
        self.push(
            None,
            quad.bounds.inflate(1.0).intersect(&quad.clip),
            Instance {
                bounds: quad.bounds.to_array(),
                clip: quad.clip.to_array(),
                radii: quad.corner_radii.to_array(),
                params: quad.border_widths.to_array(),
                color: unorm(quad.background),
                border_color: unorm(quad.border_color),
                flags: backing,
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
                radii: shadow.corner_radii.to_array(),
                params: [shadow.blur_radius.max(0.0) * 0.5, 0.0, 0.0, 0.0],
                color: unorm(shadow.color),
                flags: SHADOW | if shadow.backing { LAYER_RADII } else { 0 },
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
                params: [
                    mask.x as f32,
                    mask.y as f32,
                    mask.width as f32,
                    mask.height as f32,
                ],
                color: unorm(glyph.color),
                flags: GLYPH,
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
        let mask = if image.format() == ImageFormat::Alpha8 {
            ALPHA_MASK
        } else {
            0
        };
        self.push(
            Some(image),
            bounds.inflate(1.0).intersect(&clip),
            Instance {
                bounds: bounds.to_array(),
                clip: clip.to_array(),
                radii: corner_radii.to_array(),
                params: source.to_array(),
                color: unorm(tint),
                flags: IMAGE | mask,
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
        self.revision += 1;
    }

    pub(crate) fn instances(&self) -> &[Instance] {
        &self.instances
    }

    pub(crate) fn batches(&self) -> &[Batch] {
        &self.batches
    }
}
