use parley::fontique::Blob;
use parley::{
    Affinity, Alignment, AlignmentOptions, BoundingBox, Cluster, Cursor, FontContext, FontData,
    FontFamily, FontStyle, FontWeight, Layout, LayoutContext, LineHeight, PositionedLayoutItem,
    Selection, StyleProperty,
};
use rouge_render::Color;
use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::{Range, Sub};
use swash::FontRef;
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Angle, Format, Transform, Vector};

pub const SUBPIXEL_BUCKETS: u8 = 4;
const SHAPE_CACHE_LIMIT: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Underline {
    pub thickness: f32,
    pub color: Color,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpanStyle {
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub weight: Option<f32>,
    pub italic: Option<bool>,
    pub underline: Option<Underline>,
    pub strikethrough: Option<bool>,
    pub family: Option<Cow<'static, str>>,
}

impl SpanStyle {
    fn overlay(&mut self, other: &Self) {
        macro_rules! fields {
            ($($field:ident),*) => {$(
                if other.$field.is_some() {
                    self.$field.clone_from(&other.$field);
                }
            )*};
        }
        fields!(
            color,
            background,
            weight,
            italic,
            underline,
            strikethrough,
            family
        );
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: Cow<'static, str>,
    pub size: f32,
    pub weight: f32,
    pub italic: bool,
    pub line_height: f32,
    pub align: TextAlign,
    pub wrap: bool,
    pub spans: Vec<(Range<usize>, SpanStyle)>,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            family: Cow::Borrowed("system-ui"),
            size: 14.0,
            weight: 400.0,
            italic: false,
            line_height: 1.4,
            align: TextAlign::Start,
            wrap: true,
            spans: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
    Justify,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub font_id: u64,
    pub font_index: u32,
    pub glyph_id: u32,
    pub size_bits: u32,
    pub subpixel: u8,
    pub variation: u64,
}

pub struct PositionedGlyph<'a> {
    pub glyph_id: u32,
    pub x: f32,
    pub y: f32,
    pub font: &'a FontData,
    pub font_size: f32,
    pub style: &'a SpanStyle,
    coords: &'a [i16],
    embolden: bool,
    skew: Option<f32>,
}

impl PositionedGlyph<'_> {
    pub fn snap(&self, origin: (f32, f32), scale: f32) -> (GlyphKey, i32, i32) {
        let x = (origin.0 + self.x) * scale;
        let y = (origin.1 + self.y) * scale;
        let buckets = f32::from(SUBPIXEL_BUCKETS);
        let snapped = (x * buckets).round() / buckets;
        let px = snapped.floor();
        let subpixel = ((snapped - px) * buckets).round() as u8 % SUBPIXEL_BUCKETS;
        let mut variation = 0xcbf2_9ce4_8422_2325u64;
        let mut mix = |v: u64| variation = (variation ^ v).wrapping_mul(0x0100_0000_01b3);
        for coord in self.coords {
            mix(u64::from(*coord as u16));
        }
        mix(u64::from(self.embolden));
        mix(self.skew.map_or(0, |s| u64::from(s.to_bits()) + 1));
        let key = GlyphKey {
            font_id: self.font.data.id(),
            font_index: self.font.index,
            glyph_id: self.glyph_id,
            size_bits: (self.font_size * scale).to_bits(),
            subpixel,
            variation,
        };
        (key, px as i32, y.round() as i32)
    }
}

pub struct GlyphImage {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Movement {
    Left,
    Right,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    Up,
    Down,
    Start,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Granularity {
    Caret,
    Word,
    Line,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextSelection(Selection);

impl TextSelection {
    pub fn range(&self) -> Range<usize> {
        self.0.text_range()
    }

    pub fn is_collapsed(&self) -> bool {
        self.0.is_collapsed()
    }

    pub fn anchor(&self) -> usize {
        self.0.anchor().index()
    }

    pub fn focus(&self) -> usize {
        self.0.focus().index()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl From<BoundingBox> for TextRect {
    fn from(bounds: BoundingBox) -> Self {
        TextRect {
            x: bounds.x0 as f32,
            y: bounds.y0 as f32,
            width: (bounds.x1 - bounds.x0) as f32,
            height: (bounds.y1 - bounds.y0) as f32,
        }
    }
}

#[derive(Clone)]
pub struct TextLayout {
    layout: Layout<SpanStyle>,
    align: TextAlign,
    wrap: bool,
    len: usize,
    #[expect(clippy::option_option)]
    broken_at: Option<Option<f32>>,
}

impl TextLayout {
    pub fn index_at(&self, x: f32, y: f32) -> Option<usize> {
        Cluster::from_point_exact(&self.layout, x, y).map(|(cluster, _)| cluster.text_range().start)
    }

    pub fn for_each_rect(&self, foreground: bool, mut f: impl FnMut(TextRect, Option<Color>)) {
        for line in self.layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let style = &run.style().brush;
                let metrics = line.metrics();
                let rect = |y, height| TextRect {
                    x: run.offset(),
                    y,
                    width: run.advance(),
                    height,
                };
                if foreground {
                    if let Some(underline) = style.underline {
                        f(
                            rect(run.baseline() + 1.0, underline.thickness),
                            Some(underline.color),
                        );
                    }
                    if style.strikethrough == Some(true) {
                        f(
                            rect(run.baseline() - run.run().font_size() * 0.3, 1.0),
                            style.color,
                        );
                    }
                } else if let Some(color) = style.background {
                    f(
                        rect(
                            metrics.block_min_coord,
                            metrics.block_max_coord - metrics.block_min_coord,
                        ),
                        Some(color),
                    );
                }
            }
        }
    }

    fn cursor(&self, index: usize) -> Cursor {
        Cursor::from_byte_index(&self.layout, index.min(self.len), Affinity::Downstream)
    }

    pub fn select(&self, anchor: usize, focus: usize) -> TextSelection {
        TextSelection(Selection::new(self.cursor(anchor), self.cursor(focus)))
    }

    pub fn select_at(&self, x: f32, y: f32, granularity: Granularity) -> TextSelection {
        TextSelection(match granularity {
            Granularity::Caret => Selection::from_point(&self.layout, x, y),
            Granularity::Word => Selection::word_from_point(&self.layout, x, y),
            Granularity::Line => Selection::line_from_point(&self.layout, x, y),
        })
    }

    pub fn extend_selection(
        &self,
        selection: TextSelection,
        x: f32,
        y: f32,
        shift: bool,
    ) -> TextSelection {
        TextSelection(if shift {
            selection.0.shift_click_extension(&self.layout, x, y)
        } else {
            selection.0.extend_to_point(&self.layout, x, y)
        })
    }

    pub fn move_selection(
        &self,
        selection: TextSelection,
        movement: Movement,
        extend: bool,
    ) -> TextSelection {
        let (layout, from) = (&self.layout, selection.0);
        let to = |index: usize| {
            if extend {
                from.extend(self.cursor(index))
            } else {
                self.cursor(index).into()
            }
        };
        TextSelection(match movement {
            Movement::Left => from.previous_visual(layout, extend),
            Movement::Right => from.next_visual(layout, extend),
            Movement::WordLeft => from.previous_visual_word(layout, extend),
            Movement::WordRight => from.next_visual_word(layout, extend),
            Movement::LineStart => from.line_start(layout, extend),
            Movement::LineEnd => from.line_end(layout, extend),
            Movement::Up => from.previous_line(layout, extend),
            Movement::Down => from.next_line(layout, extend),
            Movement::Start => to(0),
            Movement::End => to(self.len),
        })
    }

    pub fn selection_rects(&self, selection: TextSelection) -> Vec<TextRect> {
        selection
            .0
            .geometry(&self.layout)
            .into_iter()
            .map(|(bounds, _)| bounds.into())
            .collect()
    }

    pub fn caret(&self, selection: TextSelection, width: f32) -> TextRect {
        selection.0.focus().geometry(&self.layout, width).into()
    }

    pub fn break_lines(&mut self, max_width: Option<f32>) {
        let max_width = max_width.filter(|_| self.wrap);
        if self.broken_at == Some(max_width) {
            return;
        }
        self.layout.break_all_lines(max_width);
        let alignment = match self.align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
            TextAlign::Justify => Alignment::Justify,
        };
        self.layout.align(alignment, AlignmentOptions::default());
        self.broken_at = Some(max_width);
    }

    pub fn size(&self) -> (f32, f32) {
        (self.layout.width(), self.layout.height())
    }

    pub fn content_widths(&self) -> (f32, f32) {
        let widths = self.layout.calculate_content_widths();
        if self.wrap {
            (widths.min, widths.max)
        } else {
            (widths.max, widths.max)
        }
    }

    pub fn line_count(&self) -> usize {
        self.layout.len()
    }

    pub fn for_each_glyph(&self, mut f: impl FnMut(PositionedGlyph<'_>)) {
        for line in self.layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let synthesis = run.synthesis();
                let coords: Vec<i16> = run
                    .normalized_coords()
                    .iter()
                    .map(|c| c.to_bits())
                    .collect();
                for glyph in glyph_run.positioned_glyphs() {
                    f(PositionedGlyph {
                        glyph_id: glyph.id,
                        x: glyph.x,
                        y: glyph.y,
                        font: run.font(),
                        font_size: run.font_size(),
                        style: &glyph_run.style().brush,
                        coords: &coords,
                        embolden: synthesis.embolden(),
                        skew: synthesis.skew(),
                    });
                }
            }
        }
    }
}

#[derive(Default)]
pub struct TextSystem {
    fonts: FontContext,
    layouts: LayoutContext<SpanStyle>,
    scaler: ScaleContext,
    shaped: HashMap<TextStyle, HashMap<String, TextLayout>>,
    shaped_len: usize,
    stats: TextStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStats {
    pub shapes: u64,
    pub cache_hits: u64,
    pub glyphs_rasterized: u64,
}

impl Sub for TextStats {
    type Output = TextStats;

    fn sub(self, earlier: TextStats) -> TextStats {
        TextStats {
            shapes: self.shapes - earlier.shapes,
            cache_hits: self.cache_hits - earlier.cache_hits,
            glyphs_rasterized: self.glyphs_rasterized - earlier.glyphs_rasterized,
        }
    }
}

impl TextSystem {
    pub fn new() -> Self {
        TextSystem::default()
    }

    pub fn stats(&self) -> TextStats {
        self.stats
    }

    pub fn add_font(&mut self, data: Vec<u8>) -> Vec<String> {
        self.shaped.clear();
        self.shaped_len = 0;
        let families = self.fonts.collection.register_fonts(Blob::from(data), None);
        families
            .into_iter()
            .filter_map(|(id, _)| self.fonts.collection.family_name(id).map(str::to_owned))
            .collect()
    }

    pub fn layout(&mut self, text: &str, style: &TextStyle) -> TextLayout {
        if let Some(layout) = self.shaped.get(style).and_then(|texts| texts.get(text)) {
            self.stats.cache_hits += 1;
            return layout.clone();
        }
        self.stats.shapes += 1;
        let layout = self.shape(text, style);
        if self.shaped_len >= SHAPE_CACHE_LIMIT {
            self.shaped.clear();
            self.shaped_len = 0;
        }
        self.shaped_len += 1;
        self.shaped
            .entry(style.clone())
            .or_default()
            .insert(text.to_owned(), layout.clone());
        layout
    }

    fn shape(&mut self, text: &str, style: &TextStyle) -> TextLayout {
        let mut builder = self
            .layouts
            .ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::from(&*style.family)));
        builder.push_default(StyleProperty::FontSize(style.size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(style.weight)));
        if style.italic {
            builder.push_default(StyleProperty::FontStyle(FontStyle::Italic));
        }
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            style.line_height,
        )));
        let mut boundaries = vec![0, text.len()];
        for (range, _) in &style.spans {
            if range.start < range.end && range.start < text.len() {
                boundaries.extend([range.start, range.end.min(text.len())]);
            }
        }
        boundaries.retain(|index| text.is_char_boundary(*index));
        boundaries.sort_unstable();
        boundaries.dedup();
        for pair in boundaries.windows(2) {
            let range = pair[0]..pair[1];
            let mut span = SpanStyle::default();
            for (source, style) in &style.spans {
                if source.start <= range.start && source.end >= range.end {
                    span.overlay(style);
                }
            }
            if let Some(weight) = span.weight {
                builder.push(
                    StyleProperty::FontWeight(FontWeight::new(weight)),
                    range.clone(),
                );
            }
            if let Some(italic) = span.italic {
                builder.push(
                    StyleProperty::FontStyle(if italic {
                        FontStyle::Italic
                    } else {
                        FontStyle::Normal
                    }),
                    range.clone(),
                );
            }
            if let Some(family) = &span.family {
                builder.push(
                    StyleProperty::FontFamily(FontFamily::from(&**family)),
                    range.clone(),
                );
            }
            builder.push(StyleProperty::Brush(span), range);
        }
        TextLayout {
            layout: builder.build(text),
            align: style.align,
            wrap: style.wrap,
            len: text.len(),
            broken_at: None,
        }
    }

    pub fn rasterize(&mut self, glyph: &PositionedGlyph<'_>, key: &GlyphKey) -> Option<GlyphImage> {
        self.stats.glyphs_rasterized += 1;
        let font = FontRef::from_index(glyph.font.data.as_ref(), glyph.font.index as usize)?;
        let mut scaler = self
            .scaler
            .builder(font)
            .size(f32::from_bits(key.size_bits))
            .hint(false)
            .normalized_coords(glyph.coords)
            .build();
        let offset = Vector::new(f32::from(key.subpixel) / f32::from(SUBPIXEL_BUCKETS), 0.0);
        let mut render = Render::new(&[Source::Outline]);
        render.format(Format::Alpha).offset(offset);
        if glyph.embolden {
            render.embolden(f32::from_bits(key.size_bits) / 24.0);
        }
        if let Some(angle) = glyph.skew {
            render.transform(Some(Transform::skew(
                Angle::from_degrees(angle),
                Angle::from_degrees(0.0),
            )));
        }
        let image = render.render(&mut scaler, glyph.glyph_id as u16)?;
        if image.content != Content::Mask
            || image.placement.width == 0
            || image.placement.height == 0
        {
            return None;
        }
        Some(GlyphImage {
            left: image.placement.left,
            top: image.placement.top,
            width: image.placement.width,
            height: image.placement.height,
            data: image.data,
        })
    }
}

impl Eq for TextStyle {}

impl Hash for TextStyle {
    fn hash<T: Hasher>(&self, state: &mut T) {
        self.family.hash(state);
        [self.size, self.weight, self.line_height]
            .map(f32::to_bits)
            .hash(state);
        (self.italic, self.align, self.wrap).hash(state);
        for (range, span) in &self.spans {
            range.hash(state);
            [span.color, span.background]
                .map(|c| c.map(|c| c.to_array().map(f32::to_bits)))
                .hash(state);
            span.weight.map(f32::to_bits).hash(state);
            (span.italic, span.strikethrough, &span.family).hash(state);
            span.underline
                .map(|u| (u.thickness.to_bits(), u.color.to_array().map(f32::to_bits)))
                .hash(state);
        }
    }
}
