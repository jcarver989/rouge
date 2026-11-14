#![allow(dead_code)]

use rouge_render::headless::{Headless, Image};
use rouge_render::{
    Atlas, Color, DrawList, EffectDraw, Glass, GpuError, Item, LayerDraw, Quad, Rect, RenderStats,
    Viewport,
};
use std::error::Error;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const WHITE: [u8; 4] = [255, 255, 255, 255];
pub(crate) const RED: [u8; 4] = [255, 0, 0, 255];
pub(crate) const BLUE: [u8; 4] = [0, 0, 255, 255];
pub(crate) const CLEAR: [u8; 4] = [0, 0, 0, 0];

pub(crate) struct Canvas {
    headless: Headless,
    atlas: Atlas,
    clear: Color,
}

impl Canvas {
    pub(crate) fn new(size: u32, scale: f32, clear: Color) -> Result<Self, GpuError> {
        Ok(Canvas {
            headless: Headless::new(Viewport::new(size, size, scale))?,
            atlas: Atlas::new(),
            clear,
        })
    }

    pub(crate) fn atlas(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    pub(crate) fn frame(&mut self, layers: &[LayerDraw<'_>]) -> RenderStats {
        let items: Vec<_> = layers.iter().copied().map(Item::Layer).collect();
        self.headless.render(self.clear, &items, &mut self.atlas)
    }

    pub(crate) fn image(&self) -> Result<Image, GpuError> {
        self.headless.image()
    }
}

pub(crate) fn render_layers(size: u32, layers: &[LayerDraw<'_>]) -> Result<Image, GpuError> {
    let mut canvas = Canvas::new(size, 1.0, Color::TRANSPARENT)?;
    canvas.frame(layers);
    canvas.image()
}

pub(crate) fn over(scene: &DrawList, glass: EffectDraw) -> Result<Image, GpuError> {
    let empty = DrawList::new();
    let glass = LayerDraw {
        backdrop: Some(glass),
        ..LayerDraw::new(&empty)
    };
    render_layers(100, &[LayerDraw::new(scene), glass])
}

pub(crate) fn quads(quads: impl IntoIterator<Item = Quad>) -> DrawList {
    let mut list = DrawList::new();
    quads.into_iter().for_each(|quad| list.push_quad(quad));
    list
}

pub(crate) fn full(color: Color) -> Quad {
    Quad::new(Rect::new(0.0, 0.0, 100.0, 100.0), color)
}

pub(crate) fn split_scene() -> DrawList {
    quads([
        Quad::new(Rect::new(0.0, 0.0, 50.0, 100.0), Color::BLACK),
        Quad::new(Rect::new(50.0, 0.0, 50.0, 100.0), Color::WHITE),
    ])
}

pub(crate) fn backdrop(bounds: Rect, blur: f32) -> EffectDraw {
    EffectDraw::new(&Glass::frosted(blur).edge_width(12.0), bounds)
}
