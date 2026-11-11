use crate::event::Modifiers;
use crate::path::Path;
use crate::prop::{IntoProp, Prop};
use crate::styled;
use crate::tree::ElementId;
use crate::view::{BuildCx, Div, View, div, send};
use rouge_macros::component;
use rouge_reactive::{Signal, effect, signal};
use rouge_render::{Color, ImageData, ImageQuad, Point, Quad, Rect, Size};
use rouge_text::{TextLayout, TextStyle};
use std::rc::Rc;

pub(crate) type HitListener = Rc<dyn Fn(Option<TextHit>, Modifiers)>;

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
    pub tag: Option<u32>,
    pub name: Option<String>,
    pub right_aligned: bool,
}

impl TextRun {
    pub fn new(content: impl Into<String>, origin: Point, style: &TextStyle, color: Color) -> Self {
        TextRun {
            content: content.into(),
            style: style.clone(),
            origin,
            color,
            clip: None,
            tag: None,
            name: None,
            right_aligned: false,
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

    pub fn tag(mut self, tag: u32) -> Self {
        self.tag = Some(tag);
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

pub(crate) struct ShapedRun {
    pub(crate) run: TextRun,
    pub(crate) layout: Option<Box<TextLayout>>,
}

impl ShapedRun {
    pub(crate) fn bounds(&self) -> Rect {
        let (width, height) = self
            .layout
            .as_ref()
            .map_or((0.0, 0.0), |layout| layout.size());
        Rect {
            origin: self.run.origin,
            size: Size::new(width, height),
        }
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
}

impl Canvas {
    pub fn element(mut self, configure: impl FnOnce(Div) -> Div) -> Self {
        self.div = configure(self.div);
        self
    }

    pub fn on_text_hover(mut self, f: impl Fn(Option<TextHit>, Modifiers) + 'static) -> Self {
        self.div.spec.text_hover = Some(Rc::new(f));
        self
    }

    pub fn on_text_click(mut self, f: impl Fn(Option<TextHit>, Modifiers) + 'static) -> Self {
        self.div.spec.text_click = Some(Rc::new(f));
        self
    }

    pub fn track_overflow(mut self, overflow: Signal<f32>) -> Self {
        self.div.spec.overflow_observer = Some(overflow);
        self
    }
}

#[component]
pub fn canvas(draw: impl Fn(&mut Painter) + 'static) -> Canvas {
    Canvas {
        div: div(),
        draw: Box::new(draw),
    }
}

styled!(Canvas => div);

impl View for Canvas {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let Canvas { mut div, draw } = self;
        let size = signal(Size::default());
        div.spec.size_observer = Some(size);
        div.spec.canvas = Some(Vec::new());
        let id = div.build(cx);
        let patches = cx.patches.clone();
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
            send(&patches, move |cx| {
                cx.tree.set_canvas(id, painter.ops, cx.text);
            });
        });
        id
    }
}

pub struct Icon {
    canvas: Div,
    path: Path,
    view_box: Size,
    color: Prop<Color>,
    stroke: Option<(f32, bool)>,
}

#[component]
pub fn icon(path: Path) -> Icon {
    Icon {
        canvas: div(),
        path,
        view_box: Size::new(24.0, 24.0),
        color: Prop::Static(Color::BLACK),
        stroke: None,
    }
}

impl Icon {
    pub fn view_box(mut self, width: f32, height: f32) -> Self {
        self.view_box = Size::new(width, height);
        self
    }

    pub fn color<U>(mut self, color: impl IntoProp<Color, U>) -> Self {
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
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let Icon {
            canvas,
            path,
            view_box,
            color,
            stroke,
        } = self;
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
            match stroke {
                Some((width, true)) => painter.stroke_round(&fitted, width * scale, color.get()),
                Some((width, false)) => painter.stroke(&fitted, width * scale, color.get()),
                None => painter.fill(&fitted, color.get()),
            }
        };
        Canvas {
            div: canvas,
            draw: Box::new(draw),
        }
        .build(cx)
    }
}
