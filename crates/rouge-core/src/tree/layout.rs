use super::content::TextData;
use super::layers::{LayerRef, LayerState, Slot};
use super::paint::layer_mut;
use super::{ElementId, Kind, Tree};
use crate::canvas::CanvasMeasure;
use crate::image::ImageSpec;
use crate::style::{Dirty, base_layout};
use crate::text::TextSystem;
use rouge_render::{Edges, Point, Rect, Size};
use std::mem::replace;
use taffy::{
    AvailableSpace, Dimension, OofPositioningArea, Point as Offset, RunMode, Size as Extent,
    Style as Layout, compute_leaf_layout,
};
use tracing::trace_span;

const WRAP_TOLERANCE: f32 = 1.0;

#[derive(Clone, Copy)]
struct Space {
    origin: Point,
    layer: LayerRef,
}

struct LayoutCx<'a> {
    text_system: &'a mut TextSystem,
    moved: Vec<LayerRef>,
}

impl Tree {
    pub(crate) fn layout(&mut self, text_system: &mut TextSystem) -> bool {
        if !self.dirty.needs_layout() {
            return false;
        }
        let _layout = trace_span!("layout").entered();
        let root = self.elements[self.root].taffy;
        let elements = &mut self.elements;
        let available = Extent {
            width: AvailableSpace::Definite(self.size.width),
            height: AvailableSpace::Definite(self.size.height),
        };
        self.taffy
            .compute_layout_with_measure(root, available, |inputs, _, context, style| {
                let kind = context
                    .and_then(|id| elements.get_mut(*id))
                    .map(|e| &mut e.kind);
                let mut output = compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, available| match kind {
                        Some(Kind::Text(text)) => measure_text(text, text_system, known, available),
                        Some(Kind::Image(image)) => measure_image(image, known),
                        Some(Kind::Canvas(canvas)) => match &mut canvas.measure {
                            Some(measure) => measure_canvas(measure, text_system, known, available),
                            None => Extent::ZERO,
                        },
                        _ => Extent::ZERO,
                    },
                );
                if inputs.run_mode == RunMode::PerformLayout {
                    output.oof_positioning_area = Some(OofPositioningArea {
                        size: output.size,
                        offset: Offset::ZERO,
                    });
                }
                output
            })
            .expect("taffy layout");
        let mut cx = LayoutCx {
            text_system,
            moved: Vec::new(),
        };
        let space = Space {
            origin: Point::ZERO,
            layer: (self.root, Slot::Own),
        };
        self.resolve_bounds(self.root, Point::ZERO, space, &mut cx);
        for layer in cx.moved {
            if let Some(layer) = layer_mut(&mut self.elements, layer) {
                layer.dirty = true;
            }
        }
        self.stats.layouts += 1;
        self.dirty = Dirty::Paint;
        true
    }

    fn resolve_bounds(
        &mut self,
        id: ElementId,
        parent_origin: Point,
        mut space: Space,
        cx: &mut LayoutCx<'_>,
    ) {
        let element = &mut self.elements[id];
        let layout = self.taffy.layout(element.taffy).expect("taffy layout");
        let (padding, border) = (layout.padding, layout.border);
        if let Some(text) = element.kind.text_mut() {
            let content_width =
                layout.size.width - padding.left - padding.right - border.left - border.right;
            let content_height =
                layout.size.height - padding.top - padding.bottom - border.top - border.bottom;
            text.inset = Point::new(padding.left + border.left, padding.top + border.top);
            text.wrap_width = Some(content_width + WRAP_TOLERANCE);
            text.content_size = Size::new(content_width.max(0.0), content_height.max(0.0));
            let wrap_width = text.wrap_width;
            text.laid_out(cx.text_system, wrap_width);
            if let (Some(edit), Some(layout)) = (&mut text.edit, &text.layout) {
                edit.selection = edit.current(layout, &text.content);
                edit.stale = false;
                edit.scroll_to_caret(layout, &text.content, text.content_size);
            }
        }
        let origin = parent_origin + Point::new(layout.location.x, layout.location.y);
        let size = Size::new(layout.size.width, layout.size.height);
        let border = Edges {
            top: border.top,
            right: border.right,
            bottom: border.bottom,
            left: border.left,
        };
        element.bounds = Rect { origin, size };
        let within = |space: &Space| Rect {
            origin: origin - space.origin,
            size,
        };
        let mut enter = |layer: &mut Option<Box<LayerState>>, slot: Slot, space: &mut Space| {
            let layer = layer.as_mut()?;
            let frame = within(space);
            if layer.frame.replace(frame) != Some(frame) {
                self.compositor.set_frame(layer.id, frame);
            }
            *space = Space {
                origin,
                layer: (id, slot),
            };
            Some(layer.id)
        };
        let [own, content] = &mut element.layers;
        enter(own, Slot::Own, &mut space);
        let painted = within(&space);
        if replace(&mut element.painted, painted) != painted || element.border != border {
            cx.moved.push(space.layer);
        }
        element.border = border;
        if let Some(scroller) = enter(content, Slot::Content, &mut space)
            && let Some(scroll) = element.scroll_mut()
        {
            let viewport = Size::new(
                (size.width - border.left - border.right).max(0.0),
                (size.height - border.top - border.bottom).max(0.0),
            );
            let content = Size::new(
                viewport.width + layout.scroll_width(),
                viewport.height + layout.scroll_height(),
            );
            if replace(&mut scroll.extent, (viewport, content)) != (viewport, content) {
                self.compositor
                    .set_scroll_extent(scroller, viewport, content);
                if let Some(target) = scroll.target {
                    self.compositor.scroll_to(scroller, target);
                }
            }
        }
        for i in 0..element.children.len() {
            let child = self.elements[id].children[i];
            self.resolve_bounds(child, origin, space, cx);
        }
    }
}

pub(super) fn root_layout(size: Size) -> Layout {
    Layout {
        size: Extent {
            width: Dimension::length(size.width),
            height: Dimension::length(size.height),
        },
        ..base_layout()
    }
}

fn measure_text(
    text: &mut TextData,
    system: &mut TextSystem,
    known: Extent<Option<f32>>,
    available: Extent<AvailableSpace>,
) -> Extent<f32> {
    let constraint = known.width.or(match available.width {
        AvailableSpace::Definite(width) => Some(width),
        AvailableSpace::MinContent => Some(text.shaped(system).content_widths().0),
        AvailableSpace::MaxContent => None,
    });
    let (width, mut height) = text.laid_out(system, constraint).size();
    if let Some(edit) = &text.edit {
        let row = text.style.size * text.style.line_height;
        height = height.clamp(
            row * edit.min_rows as f32,
            row * edit.max_rows.max(edit.min_rows) as f32,
        );
    }
    Extent {
        width: known.width.unwrap_or(width.ceil()),
        height: known.height.unwrap_or(height.ceil()),
    }
}

fn measure_canvas(
    measure: &mut CanvasMeasure,
    system: &mut TextSystem,
    known: Extent<Option<f32>>,
    available: Extent<AvailableSpace>,
) -> Extent<f32> {
    let width = known.width.or(match available.width {
        AvailableSpace::Definite(width) => Some(width),
        AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
    });
    let height = known
        .height
        .unwrap_or_else(|| measure.height(system, width.unwrap_or(f32::INFINITY)));
    if let (Some(width), None) = (width, known.height) {
        measure.last = Some(Size::new(width, height));
    }
    Extent {
        width: known.width.unwrap_or(0.0),
        height,
    }
}

fn measure_image(image: &ImageSpec, known: Extent<Option<f32>>) -> Extent<f32> {
    let natural = image.data.as_ref().map_or((0.0, 0.0), |data| {
        (data.width() as f32, data.height() as f32)
    });
    let ratio = if natural.1 > 0.0 {
        natural.0 / natural.1
    } else {
        1.0
    };
    let (width, height) = match (known.width, known.height) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) => (width, width / ratio),
        (None, Some(height)) => (height * ratio, height),
        (None, None) => natural,
    };
    Extent { width, height }
}
