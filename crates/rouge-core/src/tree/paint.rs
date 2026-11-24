use super::content::TextData;
use super::edit::CARET_WIDTH;
use super::layers::{LayerRef, LayerState, Slot};
use super::{Element, ElementId, Kind, Tree, theme_at};
use crate::canvas::{PaintOp, ShapedRun};
use crate::compositor::{Content, Layers, Nested};
use crate::image::{Fit, ImageSpec};
use crate::path::Path;
use crate::style::Dirty;
use crate::text::{GlyphKey, TextLayout, TextRect, TextSelection, TextStyle, TextSystem, TextWrap};
use crate::theme::Palette;
use rouge_render::{
    Atlas, AtlasRect, Color, Corners, DrawList, Edges, Glyph, ImageData, ImageQuad, Point, Quad,
    Rect, Shadow, Size,
};
use rustc_hash::FxHashMap;
use slotmap::SlotMap;
use std::mem::{replace, take};
use tracing::trace_span;
use zeno::{Cap, Fill, Join, Mask, Stroke, Transform};

const PATH_CACHE_LIMIT: usize = 1024;
const PATH_BUCKETS: f32 = 4.0;

#[derive(Clone, Copy)]
struct CachedGlyph {
    mask: AtlasRect,
    left: i32,
    top: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PathKey {
    path: u64,
    stroke: Option<(u32, bool)>,
    scale: u32,
    offset: (u8, u8),
}

pub(crate) struct PaintCache {
    pub atlas: Atlas,
    pub scale: f32,
    glyphs: FxHashMap<GlyphKey, Option<CachedGlyph>>,
    paths: FxHashMap<PathKey, Option<(ImageData, i32, i32)>>,
}

impl PaintCache {
    pub(crate) fn new(scale: f32) -> Self {
        PaintCache {
            atlas: Atlas::new(),
            scale,
            glyphs: FxHashMap::default(),
            paths: FxHashMap::default(),
        }
    }
}

struct PaintCx<'a> {
    cache: &'a mut PaintCache,
    text_system: &'a mut TextSystem,
    repaint_all: bool,
    layer_paints: u64,
    focused: Option<ElementId>,
    palette: Palette,
    fallback: Palette,
}

struct LayerTarget {
    root: LayerRef,
    origin: Point,
    lists: Vec<DrawList>,
    nested: Vec<(LayerRef, Option<Rect>)>,
}

impl LayerTarget {
    fn out(&mut self) -> &mut DrawList {
        if self.lists.len() <= self.nested.len() {
            self.lists.resize_with(self.nested.len() + 1, DrawList::new);
        }
        &mut self.lists[self.nested.len()]
    }

    fn nest(&mut self, layer: LayerRef, clip: Rect) {
        self.nested
            .push((layer, (clip != Rect::EVERYTHING).then_some(clip)));
    }
}

impl Tree {
    pub(crate) fn paint(&mut self, cache: &mut PaintCache, text_system: &mut TextSystem) -> bool {
        self.sync_themes();
        self.resolve_styles();
        let dirty = replace(&mut self.dirty, Dirty::None);
        if dirty.needs_layout() {
            self.dirty = Dirty::Layout;
        }
        if dirty.needs_paint() {
            let _paint = trace_span!("paint").entered();
            let repaint_all = take(&mut self.repaint_all);
            let focused = self.focused;
            let mut cx = PaintCx {
                cache,
                text_system,
                repaint_all,
                layer_paints: 0,
                focused,
                palette: self.palette_at(self.root),
                fallback: self.palette_at(self.root),
            };
            paint_layers(
                &mut self.elements,
                (self.root, Slot::Own),
                &mut cx,
                &mut self.layers,
            );
            self.stats.layer_paints += cx.layer_paints;
            self.stats.paints += 1;
        }
        dirty != Dirty::None
    }
}

fn layer_ref(elements: &SlotMap<ElementId, Element>, (id, slot): LayerRef) -> Option<&LayerState> {
    elements.get(id)?.layer(slot)
}

pub(super) fn layer_mut(
    elements: &mut SlotMap<ElementId, Element>,
    (id, slot): LayerRef,
) -> Option<&mut LayerState> {
    elements.get_mut(id)?.layers[slot as usize].as_deref_mut()
}

fn paint_layers(
    elements: &mut SlotMap<ElementId, Element>,
    layer: LayerRef,
    cx: &mut PaintCx<'_>,
    layers: &mut Layers,
) {
    let Some(state) = layer_mut(elements, layer) else {
        return;
    };
    let layer_id = state.id;
    if replace(&mut state.dirty, false) || cx.repaint_all {
        let (id, slot) = layer;
        cx.palette = theme_at(elements, id).map_or(cx.fallback, |theme| theme.get_untracked());
        let element = &elements[id];
        let mut target = LayerTarget {
            root: layer,
            origin: element.bounds.origin,
            lists: Vec::new(),
            nested: Vec::new(),
        };
        match slot {
            Slot::Own => paint(elements, id, Rect::EVERYTHING, &mut target, cx),
            Slot::Content => paint_children(elements, id, Rect::EVERYTHING, &mut target, cx),
        }
        target
            .lists
            .resize_with(target.nested.len() + 1, DrawList::new);
        let nested = target.nested.iter().filter_map(|(nested, clip)| {
            Some(Nested {
                layer: layer_ref(elements, *nested)?.id,
                clip: *clip,
            })
        });
        let content = Content {
            nested: nested.collect(),
            lists: target.lists,
        };
        layers.set_content(layer_id, content);
        cx.layer_paints += 1;
        if let Some(state) = layer_mut(elements, layer) {
            state.nested = target
                .nested
                .into_iter()
                .map(|(nested, _)| nested)
                .collect();
        }
    }
    let nested = |elements: &SlotMap<ElementId, Element>, i: usize| {
        layer_ref(elements, layer).and_then(|state| state.nested.get(i).copied())
    };
    let mut index = 0;
    while let Some(layer) = nested(elements, index) {
        paint_layers(elements, layer, cx, layers);
        index += 1;
    }
}

fn paint(
    elements: &SlotMap<ElementId, Element>,
    id: ElementId,
    clip: Rect,
    target: &mut LayerTarget,
    cx: &mut PaintCx<'_>,
) {
    let element = &elements[id];
    if element.unseen() {
        return;
    }
    let backing = target.root == (id, Slot::Own);
    if element.layer(Slot::Own).is_some() && !backing {
        target.nest((id, Slot::Own), clip);
        return;
    }
    let outer = element
        .extras()
        .and_then(|extras| extras.theme)
        .map(|theme| replace(&mut cx.palette, theme.get_untracked()));
    paint_element(elements, id, clip, target, backing, cx);
    if let Some(outer) = outer {
        cx.palette = outer;
    }
}

fn paint_element(
    elements: &SlotMap<ElementId, Element>,
    id: ElementId,
    clip: Rect,
    target: &mut LayerTarget,
    backing: bool,
    cx: &mut PaintCx<'_>,
) {
    let element = &elements[id];
    let style = element.effective_style();
    let radii = if backing {
        Corners::ZERO
    } else {
        style.corner_radii
    };
    let bounds = Rect {
        origin: element.bounds.origin - target.origin,
        size: element.bounds.size,
    };
    for shadow in style.shadows.iter() {
        target.out().push_shadow(Shadow {
            bounds: bounds.translate(shadow.offset).inflate(shadow.spread),
            clip,
            corner_radii: radii.map(|r| {
                if backing {
                    shadow.spread
                } else {
                    (r + shadow.spread).max(0.0)
                }
            }),
            color: shadow.color,
            blur_radius: shadow.blur,
            backing,
        });
    }

    let has_border = !element.border.is_zero() && style.border_color.is_visible();
    let background = style.background.filter(|fill| backing || fill.is_visible());
    if background.is_some() || has_border {
        target.out().push_quad(Quad {
            bounds,
            clip,
            background: background.map_or(Color::TRANSPARENT, |fill| fill.resolve(&cx.palette)),
            border_color: style.border_color.resolve(&cx.palette),
            corner_radii: radii,
            border_widths: if has_border {
                element.border
            } else {
                Edges::ZERO
            },
            backing,
        });
    }

    match &element.kind {
        Kind::Image(image) => {
            if let ImageSpec {
                data: Some(data),
                fit,
            } = &**image
            {
                let (fitted, source) = fit_image(*fit, bounds, data);
                target.out().push_image(ImageQuad {
                    clip: clip.intersect(&bounds),
                    corner_radii: radii,
                    source,
                    ..ImageQuad::new(fitted, data.clone())
                });
            }
        }
        Kind::Canvas(canvas) => paint_canvas(&canvas.ops, bounds.origin, clip, target.out(), cx),
        Kind::Text(text) => {
            let focused = cx.focused == Some(id);
            paint_text(
                text,
                bounds.origin + text.inset,
                clip,
                focused,
                target.out(),
                cx,
            );
        }
        Kind::Div | Kind::Surface(_) => {}
    }

    let border = element.border;
    let padding_box = Rect::new(
        bounds.origin.x + border.left,
        bounds.origin.y + border.top,
        (bounds.size.width - border.left - border.right).max(0.0),
        (bounds.size.height - border.top - border.bottom).max(0.0),
    );
    let child_clip = if element.clips_children() {
        clip.intersect(&padding_box)
    } else {
        clip
    };
    if element.layer(Slot::Content).is_some() {
        target.nest((id, Slot::Content), child_clip);
        return;
    }
    paint_children(elements, id, child_clip, target, cx);
}

fn paint_children(
    elements: &SlotMap<ElementId, Element>,
    id: ElementId,
    clip: Rect,
    target: &mut LayerTarget,
    cx: &mut PaintCx<'_>,
) {
    if !clip.is_empty() {
        for child in &elements[id].children {
            paint(elements, *child, clip, target, cx);
        }
    }
}

fn fit_image(fit: Fit, bounds: Rect, data: &ImageData) -> (Rect, Rect) {
    let natural = Size::new(data.width() as f32, data.height() as f32);
    let full = Rect::new(0.0, 0.0, 1.0, 1.0);
    let ratio = (
        bounds.size.width / natural.width,
        bounds.size.height / natural.height,
    );
    let centered = |size: Size| {
        let origin = Point::new(
            bounds.origin.x + (bounds.size.width - size.width) / 2.0,
            bounds.origin.y + (bounds.size.height - size.height) / 2.0,
        );
        Rect { origin, size }
    };
    match fit {
        Fit::Fill => (bounds, full),
        Fit::None => (centered(natural), full),
        Fit::Contain => {
            let scale = ratio.0.min(ratio.1);
            (
                centered(Size::new(natural.width * scale, natural.height * scale)),
                full,
            )
        }
        Fit::Cover => {
            let scale = ratio.0.max(ratio.1).max(f32::MIN_POSITIVE);
            let (x, y) = (ratio.0 / scale, ratio.1 / scale);
            (bounds, Rect::new((1.0 - x) / 2.0, (1.0 - y) / 2.0, x, y))
        }
    }
}

fn path_mask(
    cache: &mut PaintCache,
    path: &Path,
    stroke: Option<(f32, bool)>,
    origin: Point,
) -> Option<(ImageData, Rect)> {
    let scale = cache.scale;
    let device = (origin.x * scale, origin.y * scale);
    let base = (device.0.floor(), device.1.floor());
    let bucket = |value: f32, base: f32| {
        ((value - base) * PATH_BUCKETS)
            .floor()
            .min(PATH_BUCKETS - 1.0)
    };
    let offset = (bucket(device.0, base.0), bucket(device.1, base.1));
    let key = PathKey {
        path: path.id(),
        stroke: stroke.map(|(width, round)| (width.to_bits(), round)),
        scale: scale.to_bits(),
        offset: (offset.0 as u8, offset.1 as u8),
    };
    if cache.paths.len() >= PATH_CACHE_LIMIT && !cache.paths.contains_key(&key) {
        cache.paths.clear();
    }
    let rendered = cache.paths.entry(key).or_insert_with(|| {
        let shift = (offset.0 / PATH_BUCKETS, offset.1 / PATH_BUCKETS);
        let transform = Transform::scale(scale, scale).then_translate(shift.0, shift.1);
        let mut mask = Mask::new(path.commands());
        mask.transform(Some(transform));
        match stroke {
            Some((width, round)) => {
                let mut stroke = Stroke::new(width);
                if round {
                    stroke.cap(Cap::Round).join(Join::Round);
                }
                mask.style(stroke)
            }
            None => mask.style(Fill::NonZero),
        };
        let (pixels, placement) = mask.render();
        let image = ImageData::alpha8(placement.width, placement.height, pixels).ok()?;
        Some((image, placement.left, placement.top))
    });
    let (image, left, top) = rendered.as_ref()?;
    let bounds = Rect::new(
        (base.0 + *left as f32) / scale,
        (base.1 + *top as f32) / scale,
        image.width() as f32 / scale,
        image.height() as f32 / scale,
    );
    Some((image.clone(), bounds))
}

fn paint_canvas(
    ops: &[PaintOp],
    origin: Point,
    clip: Rect,
    out: &mut DrawList,
    cx: &mut PaintCx<'_>,
) {
    for op in ops {
        match op {
            PaintOp::Quad(quad) => {
                let clip = clip.intersect(&quad.clip.translate(origin));
                out.push_quad(Quad {
                    bounds: quad.bounds.translate(origin),
                    clip,
                    ..*quad
                });
            }
            PaintOp::Image(image) => {
                let clip = clip.intersect(&image.clip.translate(origin));
                out.push_image(ImageQuad {
                    bounds: image.bounds.translate(origin),
                    clip,
                    ..image.clone()
                });
            }
            PaintOp::Path {
                path,
                stroke,
                color,
            } => {
                if let Some((image, bounds)) = path_mask(cx.cache, path, *stroke, origin) {
                    out.push_image(ImageQuad {
                        clip,
                        tint: *color,
                        ..ImageQuad::new(bounds, image)
                    });
                }
            }
            PaintOp::Text(ShapedRun {
                run,
                layout: Some(layout),
            }) => {
                let clip = run
                    .clip
                    .map_or(clip, |own| clip.intersect(&own.translate(origin)));
                paint_glyphs(layout, origin + run.origin, clip, run.color, out, cx);
            }
            PaintOp::Text(_) | PaintOp::Region { .. } => {}
        }
    }
}

const SELECTION_ALPHA: f32 = 0.3;

fn paint_text(
    text: &TextData,
    mut origin: Point,
    mut clip: Rect,
    focused: bool,
    out: &mut DrawList,
    cx: &mut PaintCx<'_>,
) {
    let Some(layout) = text.layout.as_ref() else {
        return;
    };
    let Some(edit) = &text.edit else {
        if let Some(range) = &text.selected {
            let color = cx.palette.accent.with_alpha(SELECTION_ALPHA);
            let selection = layout.select(range.start, range.end);
            paint_selection(layout, selection, origin, clip, color, out);
        }
        return paint_glyphs(
            layout,
            origin,
            clip,
            text.color.resolve(&cx.palette),
            out,
            cx,
        );
    };
    if text.content.is_empty() && !text.placeholder.is_empty() {
        let shaped = placeholder(text, cx.text_system);
        let color = text.placeholder_color.resolve(&cx.palette);
        paint_glyphs(&shaped, origin, clip, color, out, cx);
    }
    clip = clip.intersect(
        &Rect {
            origin,
            size: text.content_size,
        }
        .inflate(CARET_WIDTH),
    );
    origin = origin - edit.scroll;
    let rect = |rect: &TextRect| text_rect(origin, rect);
    let selection = edit.current(layout, &text.content);
    if focused {
        paint_selection(layout, selection, origin, clip, edit.selection_color, out);
    }
    paint_glyphs(
        layout,
        origin,
        clip,
        text.color.resolve(&cx.palette),
        out,
        cx,
    );
    if !focused {
        return;
    }
    for composed in edit
        .compose(layout, &text.content)
        .iter()
        .flat_map(|range| layout.selection_rects(*range))
    {
        let line = rect(&composed);
        let underline = Rect::new(line.origin.x, line.max().y - 1.5, line.size.width, 1.0);
        out.push_quad(Quad {
            clip,
            ..Quad::line(underline, text.color.resolve(&cx.palette))
        });
    }
    if selection.is_collapsed() && edit.caret_visible {
        let caret = rect(&layout.caret(selection, CARET_WIDTH));
        out.push_quad(Quad {
            clip,
            ..Quad::line(
                caret,
                edit.caret_color.unwrap_or(text.color.resolve(&cx.palette)),
            )
        });
    }
}

fn placeholder(text: &TextData, system: &mut TextSystem) -> TextLayout {
    let shaped = system.layout(&text.placeholder, &text.style, text.wrap_width);
    if shaped.size().1.floor() <= text.content_size.height {
        return shaped;
    }
    let style = TextStyle {
        wrap: TextWrap::Truncate,
        ..text.style.clone()
    };
    system.layout(&text.placeholder, &style, text.wrap_width)
}

fn paint_selection(
    layout: &TextLayout,
    selection: TextSelection,
    origin: Point,
    clip: Rect,
    color: Color,
    out: &mut DrawList,
) {
    for rect in &layout.selection_rects(selection) {
        out.push_quad(Quad {
            clip,
            ..Quad::new(text_rect(origin, rect), color)
        });
    }
}

fn text_rect(origin: Point, rect: &TextRect) -> Rect {
    Rect::new(
        origin.x + rect.x,
        origin.y + rect.y,
        rect.width,
        rect.height,
    )
}

fn paint_glyphs(
    layout: &TextLayout,
    origin: Point,
    clip: Rect,
    color: Color,
    out: &mut DrawList,
    cx: &mut PaintCx<'_>,
) {
    layout.for_each_rect(false, |rect, fill| {
        out.push_quad(Quad {
            clip,
            ..Quad::new(text_rect(origin, &rect), fill.unwrap_or(color))
        });
    });
    let PaintCache {
        atlas,
        glyphs,
        scale,
        ..
    } = &mut *cx.cache;
    let scale = *scale;
    layout.for_each_glyph(|glyph| {
        let color = glyph.style.color.unwrap_or(color);
        if color.a <= 0.0 {
            return;
        }
        let (key, pen_x, pen_y) = glyph.snap((origin.x, origin.y), scale);
        let cached = *glyphs.entry(key).or_insert_with(|| {
            let image = cx.text_system.rasterize(&glyph, &key)?;
            let mask = atlas.insert(image.width, image.height, &image.data)?;
            Some(CachedGlyph {
                mask,
                left: image.left,
                top: image.top,
            })
        });
        let Some(cached) = cached else { return };
        let origin = Point::new(
            (pen_x + cached.left) as f32 / scale,
            (pen_y - cached.top) as f32 / scale,
        );
        out.push_glyph(Glyph {
            origin,
            clip,
            color,
            mask: cached.mask,
        });
    });
    layout.for_each_rect(true, |rect, stroke| {
        out.push_quad(Quad {
            clip,
            ..Quad::line(text_rect(origin, &rect), stroke.unwrap_or(color))
        });
    });
}
