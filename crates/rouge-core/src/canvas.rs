use crate::event::Modifiers;
use crate::path::Path;
use crate::prop::{IntoProp, Prop};
use crate::styled;
use crate::text::{TextLayout, TextStyle, TextSystem};
use crate::theme::{Fill, Theme};
use crate::tree::Kind;
use crate::view::{BuildCx, Div, Patch, View, div};
use rouge_macros::component;
use rouge_reactive::{Scope, Signal, effect, run_in, signal, untrack, use_context};
use rouge_render::{Color, ImageData, ImageQuad, Point, Quad, Rect, Size};
use rouge_theme::Role;
use std::ops::Range;
use std::rc::Rc;

pub(crate) type HitListener = Rc<dyn Fn(Option<TextHit>, Modifiers)>;
type MeasureFn = Rc<dyn Fn(&mut Measure<'_>, f32) -> f32>;

#[derive(Clone)]
pub(crate) struct CanvasMeasure {
    measure: MeasureFn,
    pub(crate) last: Option<Size>,
}

#[derive(Default)]
pub(crate) struct CanvasData {
    pub ops: Vec<PaintOp>,
    pub measure: Option<CanvasMeasure>,
    pub overflow: Option<Signal<f32>>,
}

impl CanvasMeasure {
    pub(crate) fn height(&self, system: &mut TextSystem, width: f32) -> f32 {
        (self.measure)(&mut Measure { system }, width).max(0.0)
    }
}

pub struct Measure<'a> {
    system: &'a mut TextSystem,
}

impl Measure<'_> {
    pub fn text(&mut self, run: &TextRun) -> Rect {
        let (width, height) = self
            .system
            .layout(&run.content, &run.style, run.wrap)
            .size();
        let x = if run.right_aligned {
            run.origin.x - width
        } else {
            run.origin.x
        };
        Rect::new(x, run.origin.y, width, height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextHit {
    pub tag: u32,
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub content: String,
    pub style: TextStyle,
    pub origin: Point,
    pub color: Color,
    pub clip: Option<Rect>,
    pub wrap: Option<f32>,
    pub continuation: Option<Continuation>,
    pub tag: Option<u32>,
    pub name: Option<String>,
    pub right_aligned: bool,
    pub track: Option<(Range<usize>, Signal<Rect>)>,
}

impl TextRun {
    pub fn new(content: impl Into<String>, origin: Point, style: &TextStyle, color: Color) -> Self {
        TextRun {
            content: content.into(),
            style: style.clone(),
            origin,
            color,
            clip: None,
            wrap: None,
            continuation: None,
            tag: None,
            name: None,
            right_aligned: false,
            track: None,
        }
    }

    pub fn right_aligned(mut self) -> Self {
        self.right_aligned = true;
        self
    }

    pub fn clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    pub fn wrap(mut self, max_width: f32) -> Self {
        self.style.wrap = true;
        self.wrap = Some(max_width.max(0.0));
        self
    }

    pub fn continuation(mut self, mark: impl Into<String>, x: f32, color: Color) -> Self {
        self.continuation = Some(Continuation {
            mark: mark.into(),
            x,
            color,
        });
        self
    }

    pub fn tag(mut self, tag: u32) -> Self {
        self.tag = Some(tag);
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn track_range(mut self, range: Range<usize>, bounds: Signal<Rect>) -> Self {
        self.track = Some((range, bounds));
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Continuation {
    pub mark: String,
    pub x: f32,
    pub color: Color,
}

pub(crate) struct ShapedRun {
    pub(crate) run: TextRun,
    pub(crate) layout: Option<TextLayout>,
}

impl ShapedRun {
    pub(crate) fn continuation_marks(&self) -> Vec<TextRun> {
        let (Some(continuation), Some(layout)) = (&self.run.continuation, &self.layout) else {
            return Vec::new();
        };
        let style = TextStyle {
            spans: Vec::new(),
            wrap: false,
            ..self.run.style.clone()
        };
        layout
            .line_tops()
            .skip(1)
            .map(|top| {
                let origin = Point::new(continuation.x, self.run.origin.y + top);
                let mark = TextRun::new(
                    continuation.mark.clone(),
                    origin,
                    &style,
                    continuation.color,
                )
                .right_aligned();
                match &self.run.name {
                    Some(name) => mark.name(format!("{name}:continuation")),
                    None => mark,
                }
            })
            .collect()
    }

    pub(crate) fn bounds(&self) -> Rect {
        let (width, height) = self.layout.as_ref().map_or((0.0, 0.0), TextLayout::size);
        Rect {
            origin: self.run.origin,
            size: Size::new(width, height),
        }
    }

    pub(crate) fn tracked(&self) -> Option<(Signal<Rect>, Rect)> {
        let (range, bounds) = self.run.track.as_ref()?;
        let layout = self.layout.as_ref()?;
        let end = range.end.min(self.run.content.len());
        let start = range.start.min(end);
        let first = layout
            .selection_rects(layout.select(start, end))
            .into_iter()
            .next()?;
        let rect = Rect::new(
            self.run.origin.x + first.x,
            self.run.origin.y + first.y,
            first.width,
            first.height,
        );
        Some((*bounds, rect))
    }

    pub(crate) fn hit(&self, point: Point) -> Option<TextHit> {
        let tag = self.run.tag?;
        if self.run.clip.is_some_and(|clip| !clip.contains(point)) || !self.bounds().contains(point)
        {
            return None;
        }
        let local = point - self.run.origin;
        let index = self.layout.as_ref()?.index_at(local.x, local.y)?;
        Some(TextHit { tag, index })
    }

    pub(crate) fn overflow(&self) -> f32 {
        self.run
            .clip
            .map_or(0.0, |clip| (self.bounds().max().x - clip.max().x).max(0.0))
    }
}

pub(crate) enum PaintOp {
    Quad(Quad),
    Image(ImageQuad),
    Path {
        path: Path,
        stroke: Option<(f32, bool)>,
        color: Color,
    },
    Text(ShapedRun),
    Region {
        name: String,
        rect: Rect,
    },
}

pub struct Painter {
    size: Size,
    ops: Vec<PaintOp>,
}

impl Painter {
    pub fn size(&self) -> Size {
        self.size
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.quad(Quad::new(rect, color));
    }

    pub fn quad(&mut self, quad: Quad) {
        self.ops.push(PaintOp::Quad(quad));
    }

    pub fn fill(&mut self, path: &Path, color: Color) {
        self.ops.push(PaintOp::Path {
            path: path.clone(),
            stroke: None,
            color,
        });
    }

    pub fn stroke(&mut self, path: &Path, width: f32, color: Color) {
        self.ops.push(PaintOp::Path {
            path: path.clone(),
            stroke: Some((width, false)),
            color,
        });
    }

    pub fn image(&mut self, image: &ImageData, rect: Rect) {
        self.ops
            .push(PaintOp::Image(ImageQuad::new(rect, image.clone())));
    }

    pub fn stroke_round(&mut self, path: &Path, width: f32, color: Color) {
        self.ops.push(PaintOp::Path {
            path: path.clone(),
            stroke: Some((width, true)),
            color,
        });
    }

    pub fn text(
        &mut self,
        content: impl Into<String>,
        origin: Point,
        style: &TextStyle,
        color: Color,
    ) {
        self.run(TextRun::new(content, origin, style, color));
    }

    pub fn run(&mut self, run: TextRun) {
        self.ops
            .push(PaintOp::Text(ShapedRun { run, layout: None }));
    }

    pub fn region(&mut self, name: impl Into<String>, rect: Rect) {
        self.ops.push(PaintOp::Region {
            name: name.into(),
            rect,
        });
    }
}

pub struct Canvas {
    div: Div,
    draw: Box<dyn Fn(&mut Painter)>,
    data: CanvasData,
}

impl Canvas {
    pub fn element(mut self, configure: impl FnOnce(Div) -> Div) -> Self {
        self.div = configure(self.div);
        self
    }

    pub fn on_text_hover(mut self, f: impl Fn(Option<TextHit>, Modifiers) + 'static) -> Self {
        let owner = Scope::current();
        self.div.spec.extras().text_hover = Some(Rc::new(move |hit, modifiers| {
            run_in(owner, || f(hit, modifiers));
        }));
        self
    }

    pub fn on_text_click(mut self, f: impl Fn(Option<TextHit>, Modifiers) + 'static) -> Self {
        let owner = Scope::current();
        self.div.spec.extras().text_click = Some(Rc::new(move |hit, modifiers| {
            run_in(owner, || f(hit, modifiers));
        }));
        self
    }

    pub fn measure(mut self, measure: impl Fn(&mut Measure<'_>, f32) -> f32 + 'static) -> Self {
        let owner = Scope::current();
        self.data.measure = Some(CanvasMeasure {
            measure: Rc::new(move |cx, width| {
                run_in(owner, || untrack(|| measure(cx, width))).unwrap_or(0.0)
            }),
            last: None,
        });
        self
    }

    pub fn track_overflow(mut self, overflow: Signal<f32>) -> Self {
        self.data.overflow = Some(overflow);
        self
    }
}

#[component]
pub fn canvas(draw: impl Fn(&mut Painter) + 'static) -> Canvas {
    Canvas {
        div: div(),
        draw: Box::new(draw),
        data: CanvasData::default(),
    }
}

styled!(Canvas => div);

impl View for Canvas {
    fn build(self, cx: &mut BuildCx<'_>) {
        let Canvas {
            mut div,
            draw,
            data,
        } = self;
        let size = signal(Size::default());
        div.spec.extras().size_observer = Some(size);
        div.spec.kind = Kind::Canvas(Box::new(data));
        let id = div.mount(cx);
        let queue = cx.queue.clone();
        effect(move || {
            let size = size.get();
            if size.width <= 0.0 || size.height <= 0.0 {
                return;
            }
            let mut painter = Painter {
                size,
                ops: Vec::new(),
            };
            draw(&mut painter);
            queue.push(Patch::Canvas(id, painter.ops));
        });
    }
}

pub struct Icon {
    canvas: Div,
    path: Path,
    view_box: Size,
    color: Prop<Fill>,
    stroke: Option<(f32, bool)>,
}

#[component]
pub fn icon(path: Path) -> Icon {
    Icon {
        canvas: div(),
        path,
        view_box: Size::new(24.0, 24.0),
        color: Prop::Static(Fill::Role(Role::Text)),
        stroke: None,
    }
}

impl Icon {
    pub fn view_box(mut self, width: f32, height: f32) -> Self {
        self.view_box = Size::new(width, height);
        self
    }

    pub fn color<U>(mut self, color: impl IntoProp<Fill, U>) -> Self {
        self.color = color.into_prop();
        self
    }

    pub fn stroke(mut self, width: f32) -> Self {
        self.stroke = Some((width, false));
        self
    }

    pub fn stroke_round(mut self, width: f32) -> Self {
        self.stroke = Some((width, true));
        self
    }
}

styled!(Icon => canvas);

impl View for Icon {
    fn build(self, cx: &mut BuildCx<'_>) {
        let Icon {
            canvas,
            path,
            view_box,
            color,
            stroke,
        } = self;
        let theme = use_context::<Theme>();
        let draw = move |painter: &mut Painter| {
            let size = painter.size();
            if size.width <= 0.0 || size.height <= 0.0 {
                return;
            }
            let fitted = path.fit(
                view_box,
                Rect {
                    origin: Point::ZERO,
                    size,
                },
            );
            let scale = (size.width / view_box.width).min(size.height / view_box.height);
            let color = theme.map_or(Color::BLACK, |theme| theme.resolve(color.get()));
            match stroke {
                Some((width, true)) => painter.stroke_round(&fitted, width * scale, color),
                Some((width, false)) => painter.stroke(&fitted, width * scale, color),
                None => painter.fill(&fitted, color),
            }
        };
        Canvas {
            div: canvas,
            draw: Box::new(draw),
            data: CanvasData::default(),
        }
        .build(cx);
    }
}
