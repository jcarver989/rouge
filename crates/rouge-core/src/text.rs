pub(crate) mod access;

use parley::fontique::{Blob, GenericFamily};
use parley::{
    Affinity, Alignment, AlignmentOptions, BoundingBox, Cluster, Cursor, FontContext, FontData,
    FontFamily, FontStyle, FontWeight, IndentOptions, Layout, LayoutContext, Line, LineHeight,
    OverflowWrap, PositionedLayoutItem, Run, Selection, StyleProperty,
};
use rouge_render::Color;
use rustc_hash::{FxHashMap, FxHasher};
use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::mem::take;
use std::ops::{Range, Sub};
use std::rc::Rc;
use std::sync::Arc;
use swash::FontRef;
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Angle, Format, Transform, Vector};

pub const SUBPIXEL_BUCKETS: u8 = 4;
const GENERATION_LIMIT: usize = 4096;
const WIDTHS_PER_TEXT: usize = 4;

type Segment = (Range<usize>, u16);
type SpaceKey = (Cow<'static, str>, u32, u32, bool);

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
        self.color = other.color.or(self.color);
        self.background = other.background.or(self.background);
        self.weight = other.weight.or(self.weight);
        self.italic = other.italic.or(self.italic);
        self.underline = other.underline.or(self.underline);
        self.strikethrough = other.strikethrough.or(self.strikethrough);
        self.family = other.family.clone().or(self.family.take());
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: Cow<'static, str>,
    pub size: f32,
    pub weight: f32,
    pub italic: bool,
    pub strikethrough: bool,
    pub line_height: f32,
    pub align: TextAlign,
    pub wrap: TextWrap,
    pub wrap_indent: WrapIndent,
    pub tab_size: u8,
    pub spans: Vec<(Range<usize>, SpanStyle)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextWrap {
    #[default]
    Wrap,
    NoWrap,
    Truncate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WrapIndent {
    #[default]
    None,
    Leading {
        extra_columns: u8,
    },
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            family: Cow::Borrowed("system-ui"),
            size: 14.0,
            weight: 400.0,
            italic: false,
            strikethrough: false,
            line_height: 1.4,
            align: TextAlign::Start,
            wrap: TextWrap::Wrap,
            wrap_indent: WrapIndent::None,
            tab_size: 4,
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
    run: &'a Run<'a, u16>,
    variation: u64,
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
        let key = GlyphKey {
            font_id: self.font.data.id(),
            font_index: self.font.index,
            glyph_id: self.glyph_id,
            size_bits: (self.font_size * scale).to_bits(),
            subpixel,
            variation: self.variation,
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

pub(crate) fn boundary(text: &str, index: usize) -> usize {
    text.floor_char_boundary(index.min(text.len()))
}

pub fn utf16_offset(text: &str, byte: usize) -> usize {
    text[..boundary(text, byte)].encode_utf16().count()
}

pub fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, character) in text.char_indices() {
        if units + character.len_utf16() > utf16 {
            return index;
        }
        units += character.len_utf16();
    }
    text.len()
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
    shaped: Rc<Shaped>,
    paints: Rc<[SpanStyle]>,
    strikethrough: bool,
}

#[derive(Clone)]
struct Shaped {
    layout: Layout<u16>,
    width: Option<f32>,
    fits_from: Option<f32>,
    align: TextAlign,
    indent: f32,
    len: usize,
    wrap: TextWrap,
    ellipsis: Option<Rc<Ellipsis>>,
    cuts: Vec<Option<Cut>>,
}

struct Ellipsis {
    layout: Layout<u16>,
    baseline: f32,
    width: f32,
}

#[derive(Clone, Copy)]
struct Cut {
    glyphs: usize,
    x: f32,
    end: f32,
}

impl Cut {
    fn new(line: &Line<'_, u16>, width: f32, ellipsis: f32) -> Option<Self> {
        let metrics = line.metrics();
        let mut x = metrics.offset;
        if x + metrics.advance - metrics.hanging_advance <= width {
            return None;
        }
        let mut glyphs = 0;
        let mut cut = Cut { glyphs, x, end: x };
        for run in line.runs() {
            for cluster in run.visual_clusters() {
                if x + ellipsis > width {
                    return Some(cut);
                }
                if !cluster.is_ligature_continuation() {
                    cut = Cut {
                        glyphs,
                        x,
                        end: x + ellipsis,
                    };
                }
                x += cluster.advance();
                glyphs += cluster.glyphs().count();
            }
        }
        Some(cut)
    }

    fn clamp(self, mut rect: TextRect) -> Option<TextRect> {
        rect.width = rect.width.min(self.end - rect.x);
        (rect.width > 0.0).then_some(rect)
    }
}

impl Shaped {
    fn broken_width(&self, width: Option<f32>) -> Option<f32> {
        width.filter(|width| self.fits_from.is_none_or(|fits_from| *width < fits_from))
    }

    fn break_at(&mut self, width: Option<f32>) {
        let indent = width.map_or(0.0, |width| self.indent.min(width / 2.0).max(0.0));
        self.layout.set_text_indent(
            indent,
            IndentOptions {
                each_line: true,
                hanging: true,
            },
        );
        self.layout
            .break_all_lines(width.filter(|_| self.wrap == TextWrap::Wrap));
        let cut_at = width.filter(|width| self.ellipsis.is_some() && self.layout.width() > *width);
        if self.ellipsis.is_some() && cut_at.is_none() {
            self.layout.break_all_lines(width);
        }
        let alignment = match self.align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
            TextAlign::Justify => Alignment::Justify,
        };
        self.layout.align(alignment, AlignmentOptions::default());
        self.cuts = match (&self.ellipsis, cut_at) {
            (Some(ellipsis), Some(width)) => self
                .layout
                .lines()
                .map(|line| Cut::new(&line, width, ellipsis.width))
                .collect(),
            _ => Vec::new(),
        };
        self.width = width;
    }
}

impl TextLayout {
    pub(crate) fn fits(&self, max_width: Option<f32>) -> bool {
        let width = bound(max_width, self.shaped.wrap);
        self.shaped.width == self.shaped.broken_width(width)
    }

    fn cut(&self, line: usize) -> Option<Cut> {
        self.shaped.cuts.get(line).copied().flatten()
    }

    fn paint(&self, brush: u16) -> &SpanStyle {
        &self.paints[usize::from(brush)]
    }

    pub fn index_at(&self, x: f32, y: f32) -> Option<usize> {
        Cluster::from_point_exact(&self.shaped.layout, x, y)
            .map(|(cluster, _)| cluster.text_range().start)
    }

    fn is_decorated(&self, foreground: bool) -> bool {
        self.paints.iter().any(|paint| {
            if foreground {
                paint.underline.is_some() || paint.strikethrough.unwrap_or(self.strikethrough)
            } else {
                paint.background.is_some()
            }
        })
    }

    pub fn for_each_rect(&self, foreground: bool, mut f: impl FnMut(TextRect, Option<Color>)) {
        if !self.is_decorated(foreground) {
            return;
        }
        for (index, line) in self.shaped.layout.lines().enumerate() {
            let cut = self.cut(index);
            let mut f = |rect: TextRect, color| {
                if let Some(rect) = cut.map_or(Some(rect), |cut| cut.clamp(rect)) {
                    f(rect, color);
                }
            };
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let style = self.paint(run.style().brush);
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
                    if style.strikethrough.unwrap_or(self.strikethrough) {
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
        Cursor::from_byte_index(
            &self.shaped.layout,
            index.min(self.shaped.len),
            Affinity::Downstream,
        )
    }

    pub fn select(&self, anchor: usize, focus: usize) -> TextSelection {
        TextSelection(Selection::new(self.cursor(anchor), self.cursor(focus)))
    }

    pub fn select_at(&self, x: f32, y: f32, granularity: Granularity) -> TextSelection {
        let layout = &self.shaped.layout;
        TextSelection(match granularity {
            Granularity::Caret => Selection::from_point(layout, x, y),
            Granularity::Word => Selection::word_from_point(layout, x, y),
            Granularity::Line => Selection::line_from_point(layout, x, y),
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
            selection.0.shift_click_extension(&self.shaped.layout, x, y)
        } else {
            selection.0.extend_to_point(&self.shaped.layout, x, y)
        })
    }

    pub fn move_selection(
        &self,
        selection: TextSelection,
        movement: Movement,
        extend: bool,
    ) -> TextSelection {
        let (layout, from) = (&self.shaped.layout, selection.0);
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
            Movement::End => to(self.shaped.len),
        })
    }

    pub fn selection_rects(&self, selection: TextSelection) -> Vec<TextRect> {
        selection
            .0
            .geometry(&self.shaped.layout)
            .into_iter()
            .filter_map(|(bounds, line)| match self.cut(line) {
                Some(cut) => cut.clamp(bounds.into()),
                None => Some(bounds.into()),
            })
            .collect()
    }

    pub fn caret(&self, selection: TextSelection, width: f32) -> TextRect {
        selection
            .0
            .focus()
            .geometry(&self.shaped.layout, width)
            .into()
    }

    pub fn size(&self) -> (f32, f32) {
        let layout = &self.shaped.layout;
        let width = match self.shaped.width {
            Some(width) if !self.shaped.cuts.is_empty() => layout.width().min(width),
            _ => layout.width(),
        };
        (width, layout.height())
    }

    pub fn content_widths(&self) -> (f32, f32) {
        let widths = self.shaped.layout.calculate_content_widths();
        let min = match (&self.shaped.ellipsis, self.shaped.wrap) {
            (Some(ellipsis), _) => ellipsis.width.min(widths.max),
            (None, TextWrap::Wrap) => widths.min,
            (None, _) => widths.max,
        };
        (min, widths.max)
    }

    pub fn line_count(&self) -> usize {
        self.shaped.layout.len()
    }

    pub fn line_tops(&self) -> impl Iterator<Item = f32> + '_ {
        self.shaped
            .layout
            .lines()
            .map(|line| line.metrics().block_min_coord)
    }

    pub fn for_each_glyph(&self, mut f: impl FnMut(PositionedGlyph<'_>)) {
        for (index, line) in self.shaped.layout.lines().enumerate() {
            let cut = self.cut(index);
            let shown = cut.map_or(usize::MAX, |cut| cut.glyphs);
            self.for_each_glyph_in(&line, shown, (0.0, 0.0), &mut f);
            if let (Some(cut), Some(ellipsis)) = (cut, &self.shaped.ellipsis)
                && cut.end > cut.x
                && let Some(mark) = ellipsis.layout.lines().next()
            {
                let offset = (cut.x, line.metrics().baseline - ellipsis.baseline);
                self.for_each_glyph_in(&mark, usize::MAX, offset, &mut f);
            }
        }
    }

    fn for_each_glyph_in(
        &self,
        line: &Line<'_, u16>,
        mut limit: usize,
        offset: (f32, f32),
        f: &mut impl FnMut(PositionedGlyph<'_>),
    ) {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let synthesis = run.synthesis();
            let variation = variation(run);
            let style = self.paint(glyph_run.style().brush);
            for glyph in glyph_run.positioned_glyphs() {
                if limit == 0 {
                    return;
                }
                limit -= 1;
                f(PositionedGlyph {
                    glyph_id: glyph.id,
                    x: offset.0 + glyph.x,
                    y: offset.1 + glyph.y,
                    font: run.font(),
                    font_size: run.font_size(),
                    style,
                    run,
                    variation,
                    embolden: synthesis.embolden(),
                    skew: synthesis.skew(),
                });
            }
        }
    }
}

pub struct TextSystem {
    fonts: FontContext,
    layouts: LayoutContext<u16>,
    scaler: ScaleContext,
    cache: ShapeCache,
    spaces: FxHashMap<SpaceKey, f32>,
    plain: Rc<[SpanStyle]>,
    stats: TextStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStats {
    pub shapes: u64,
    pub breaks: u64,
    pub cache_hits: u64,
    pub glyphs_rasterized: u64,
}

impl Sub for TextStats {
    type Output = TextStats;

    fn sub(self, earlier: TextStats) -> TextStats {
        TextStats {
            shapes: self.shapes - earlier.shapes,
            breaks: self.breaks - earlier.breaks,
            cache_hits: self.cache_hits - earlier.cache_hits,
            glyphs_rasterized: self.glyphs_rasterized - earlier.glyphs_rasterized,
        }
    }
}

impl Default for TextSystem {
    fn default() -> Self {
        TextSystem {
            fonts: FontContext::default(),
            layouts: LayoutContext::default(),
            scaler: ScaleContext::default(),
            cache: ShapeCache::default(),
            spaces: FxHashMap::default(),
            plain: Rc::new([SpanStyle::default()]),
            stats: TextStats::default(),
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

    pub fn add_font(&mut self, data: impl Into<Cow<'static, [u8]>>) -> Vec<String> {
        self.cache = ShapeCache::default();
        self.spaces.clear();
        let blob = Blob::new(Arc::new(data.into()));
        let families = self.fonts.collection.register_fonts(blob, None);
        let collection = &mut self.fonts.collection;
        if collection
            .generic_families(GenericFamily::SystemUi)
            .next()
            .is_none()
        {
            collection.set_generic_families(
                GenericFamily::SystemUi,
                families.first().map(|(id, _)| *id).into_iter(),
            );
        }
        families
            .into_iter()
            .filter_map(|(id, _)| self.fonts.collection.family_name(id).map(str::to_owned))
            .collect()
    }

    pub fn layout(&mut self, text: &str, style: &TextStyle, max_width: Option<f32>) -> TextLayout {
        let width = bound(max_width, style.wrap);
        let (segments, palette) = if style.spans.is_empty() {
            (Vec::new(), self.plain.clone())
        } else {
            let (segments, palette) = classify(text, &style.spans);
            (segments, Rc::from(palette))
        };
        let request = Request {
            text,
            style,
            segments,
            palette,
        };
        let hash = request.hash();
        if let Some(entry) = self.cache.find(hash, &request) {
            let shaped = entry.at(width, &mut self.stats);
            let paints = if entry.palette == request.palette {
                entry.palette.clone()
            } else {
                request.palette
            };
            return TextLayout {
                shaped,
                paints,
                strikethrough: style.strikethrough,
            };
        }
        self.stats.shapes += 1;
        let mut shaped = self.shape(&request);
        shaped.break_at(shaped.broken_width(width));
        let shaped = Rc::new(shaped);
        let paints = request.palette.clone();
        self.cache.insert(hash, request.into_entry(shaped.clone()));
        TextLayout {
            shaped,
            paints,
            strikethrough: style.strikethrough,
        }
    }

    fn shape(&mut self, request: &Request<'_>) -> Shaped {
        let Request {
            text,
            style,
            segments,
            palette,
        } = request;
        let tabbed = text.contains('\t');
        let source = if tabbed {
            Cow::Owned(text.replace('\t', " "))
        } else {
            Cow::Borrowed(*text)
        };
        let stops = if tabbed {
            tab_stops(text, style.tab_size, |index| {
                let class = segment_at(segments, index);
                self.space(style, &palette[usize::from(class)])
            })
        } else {
            Vec::new()
        };
        let mut layout = self.build(&source, style, segments, palette, &stops);
        let indent = match style.wrap_indent {
            WrapIndent::Leading { extra_columns } if style.wrap == TextWrap::Wrap => {
                layout.break_all_lines(None);
                leading_width(&layout, text)
                    + f32::from(extra_columns) * self.space(style, &SpanStyle::default())
            }
            _ => 0.0,
        };
        let same_at_any_fit = style.wrap == TextWrap::Wrap
            && style.align == TextAlign::Start
            && indent == 0.0
            && !layout.is_rtl();
        let fits_from = same_at_any_fit.then(|| layout.calculate_content_widths().max);
        let ellipsis = (style.wrap == TextWrap::Truncate).then(|| Rc::new(self.ellipsis(style)));
        Shaped {
            layout,
            width: None,
            fits_from,
            align: style.align,
            indent,
            len: text.len(),
            wrap: style.wrap,
            ellipsis,
            cuts: Vec::new(),
        }
    }

    fn ellipsis(&mut self, style: &TextStyle) -> Ellipsis {
        let mut layout = self.build("…", style, &[], &[], &[]);
        layout.break_all_lines(None);
        Ellipsis {
            baseline: layout.first_baseline().unwrap_or_default(),
            width: layout.width(),
            layout,
        }
    }

    fn space(&mut self, style: &TextStyle, span: &SpanStyle) -> f32 {
        let family = span.family.as_ref().unwrap_or(&style.family).clone();
        let weight = span.weight.unwrap_or(style.weight);
        let italic = span.italic.unwrap_or(style.italic);
        let key = (family, style.size.to_bits(), weight.to_bits(), italic);
        if let Some(width) = self.spaces.get(&key) {
            return *width;
        }
        let plain = TextStyle {
            family: key.0.clone(),
            weight,
            italic,
            spans: Vec::new(),
            wrap: TextWrap::NoWrap,
            wrap_indent: WrapIndent::None,
            align: TextAlign::Start,
            ..*style
        };
        let mut layout = self.build(" ", &plain, &[], &[], &[]);
        layout.break_all_lines(None);
        let width = layout.width();
        self.spaces.insert(key, width);
        width
    }

    fn build(
        &mut self,
        text: &str,
        style: &TextStyle,
        segments: &[Segment],
        palette: &[SpanStyle],
        stops: &[(usize, f32)],
    ) -> Layout<u16> {
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
        if style.wrap == TextWrap::Wrap {
            builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::BreakWord));
        }
        for (range, class) in segments {
            let span = &palette[usize::from(*class)];
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
            builder.push(StyleProperty::Brush(*class), range.clone());
        }
        for &(index, spacing) in stops {
            builder.push(StyleProperty::LetterSpacing(spacing), index..=index);
        }
        builder.build(text)
    }

    pub fn rasterize(&mut self, glyph: &PositionedGlyph<'_>, key: &GlyphKey) -> Option<GlyphImage> {
        self.stats.glyphs_rasterized += 1;
        let font = FontRef::from_index(glyph.font.data.as_ref(), glyph.font.index as usize)?;
        let coords = glyph
            .run
            .normalized_coords()
            .iter()
            .map(|coord| coord.to_bits());
        let mut scaler = self
            .scaler
            .builder(font)
            .size(f32::from_bits(key.size_bits))
            .hint(false)
            .normalized_coords(coords)
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

struct Request<'a> {
    text: &'a str,
    style: &'a TextStyle,
    segments: Vec<Segment>,
    palette: Rc<[SpanStyle]>,
}

impl Request<'_> {
    fn hash(&self) -> u64 {
        let mut hasher = FxHasher::default();
        self.text.hash(&mut hasher);
        shape_of(self.style).hash(&mut hasher);
        self.segments.hash(&mut hasher);
        for span in self.palette.iter() {
            font_of(span).hash(&mut hasher);
        }
        hasher.finish()
    }

    fn into_entry(self, shaped: Rc<Shaped>) -> Entry {
        Entry {
            text: self.text.into(),
            style: TextStyle {
                family: self.style.family.clone(),
                spans: Vec::new(),
                ..*self.style
            },
            segments: self.segments.into(),
            palette: self.palette,
            widths: vec![shaped],
        }
    }
}

struct Entry {
    text: Box<str>,
    style: TextStyle,
    segments: Box<[Segment]>,
    palette: Rc<[SpanStyle]>,
    widths: Vec<Rc<Shaped>>,
}

impl Entry {
    fn matches(&self, request: &Request<'_>) -> bool {
        *self.text == *request.text
            && shape_of(&self.style) == shape_of(request.style)
            && *self.segments == *request.segments
            && self.palette.len() == request.palette.len()
            && self
                .palette
                .iter()
                .zip(request.palette.iter())
                .all(|(known, span)| font_of(known) == font_of(span))
    }

    fn at(&mut self, width: Option<f32>, stats: &mut TextStats) -> Rc<Shaped> {
        let width = self.widths[0].broken_width(width);
        if let Some(index) = self.widths.iter().position(|shaped| shaped.width == width) {
            stats.cache_hits += 1;
            self.widths[..=index].rotate_right(1);
            return self.widths[0].clone();
        }
        stats.breaks += 1;
        let full = self.widths.len() >= WIDTHS_PER_TEXT;
        let idle = self
            .widths
            .iter()
            .rposition(|shaped| Rc::strong_count(shaped) == 1);
        let mut shaped = match idle.filter(|_| full) {
            Some(index) => self.widths.remove(index),
            None => self.widths[0].clone(),
        };
        Rc::make_mut(&mut shaped).break_at(width);
        self.widths.insert(0, shaped.clone());
        self.widths.truncate(WIDTHS_PER_TEXT);
        shaped
    }
}

#[derive(Default)]
struct ShapeCache {
    current: FxHashMap<u64, Vec<Entry>>,
    previous: FxHashMap<u64, Vec<Entry>>,
    len: usize,
}

impl ShapeCache {
    fn find(&mut self, hash: u64, request: &Request<'_>) -> Option<&mut Entry> {
        if let Some(bucket) = self.previous.get_mut(&hash)
            && let Some(index) = bucket.iter().position(|entry| entry.matches(request))
        {
            let entry = bucket.swap_remove(index);
            if bucket.is_empty() {
                self.previous.remove(&hash);
            }
            self.insert(hash, entry);
        }
        self.current
            .get_mut(&hash)?
            .iter_mut()
            .find(|entry| entry.matches(request))
    }

    fn insert(&mut self, hash: u64, entry: Entry) {
        if self.len >= GENERATION_LIMIT {
            self.previous = take(&mut self.current);
            self.len = 0;
        }
        self.len += 1;
        self.current.entry(hash).or_default().push(entry);
    }
}

fn bound(width: Option<f32>, wrap: TextWrap) -> Option<f32> {
    width.filter(|_| wrap != TextWrap::NoWrap)
}

fn shape_of(style: &TextStyle) -> (&str, [u32; 3], bool, TextAlign, TextWrap, WrapIndent, u8) {
    (
        &style.family,
        [style.size, style.weight, style.line_height].map(f32::to_bits),
        style.italic,
        style.align,
        style.wrap,
        style.wrap_indent,
        style.tab_size,
    )
}

fn font_of(span: &SpanStyle) -> (Option<u32>, Option<bool>, Option<&str>) {
    (
        span.weight.map(f32::to_bits),
        span.italic,
        span.family.as_deref(),
    )
}

fn variation(run: &Run<'_, u16>) -> u64 {
    let synthesis = run.synthesis();
    let mut variation = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |v: u64| variation = (variation ^ v).wrapping_mul(0x0100_0000_01b3);
    for coord in run.normalized_coords() {
        mix(u64::from(coord.to_bits() as u16));
    }
    mix(u64::from(synthesis.embolden()));
    mix(synthesis.skew().map_or(0, |s| u64::from(s.to_bits()) + 1));
    variation
}

fn classify(text: &str, spans: &[(Range<usize>, SpanStyle)]) -> (Vec<Segment>, Vec<SpanStyle>) {
    let len = text.len();
    let mut boundaries: Vec<usize> = spans
        .iter()
        .filter(|(range, _)| range.start < range.end && range.start < len)
        .flat_map(|(range, _)| [range.start, range.end.min(len)])
        .chain([0, len])
        .filter(|index| text.is_char_boundary(*index))
        .collect();
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut starts: Vec<usize> = (0..spans.len()).collect();
    starts.sort_by_key(|index| spans[*index].0.start);
    let mut pending = starts.into_iter().peekable();
    let mut active: Vec<usize> = Vec::new();
    let mut palette = vec![SpanStyle::default()];
    let mut segments: Vec<Segment> = Vec::new();
    for pair in boundaries.windows(2) {
        let range = pair[0]..pair[1];
        while let Some(index) = pending.next_if(|index| spans[*index].0.start <= range.start) {
            let at = active.partition_point(|active| *active < index);
            active.insert(at, index);
        }
        active.retain(|index| spans[*index].0.end >= range.end);
        let mut style = SpanStyle::default();
        for index in &active {
            style.overlay(&spans[*index].1);
        }
        let known = palette.iter().position(|known| *known == style);
        let class = known.unwrap_or_else(|| {
            palette.push(style);
            palette.len() - 1
        }) as u16;
        match segments.last_mut() {
            Some((last, previous)) if *previous == class && last.end == range.start => {
                last.end = range.end;
            }
            _ if class != 0 => segments.push((range, class)),
            _ => {}
        }
    }
    (segments, palette)
}

fn segment_at(segments: &[Segment], index: usize) -> u16 {
    let at = segments.partition_point(|(range, _)| range.end <= index);
    segments
        .get(at)
        .filter(|(range, _)| range.start <= index)
        .map_or(0, |(_, class)| *class)
}

fn leading_width(layout: &Layout<u16>, text: &str) -> f32 {
    text.char_indices()
        .take_while(|(_, c)| *c == ' ' || *c == '\t')
        .map(|(index, _)| Cluster::from_byte_index(layout, index).map_or(0.0, |c| c.advance()))
        .sum()
}

fn tab_stops(text: &str, tab_size: u8, mut space: impl FnMut(usize) -> f32) -> Vec<(usize, f32)> {
    let tab_size = usize::from(tab_size);
    let mut column = 0;
    let mut stops = Vec::new();
    for (index, c) in text.char_indices() {
        match c {
            '\n' => column = 0,
            '\t' => {
                let columns = if tab_size == 0 {
                    0
                } else {
                    tab_size - column % tab_size
                };
                let advance = space(index);
                stops.push((index, advance * columns as f32 - advance));
                column += columns;
            }
            _ => column += 1,
        }
    }
    stops
}
