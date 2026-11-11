use crate::path::Path;
use crate::prop::{IntoProp, Prop};
use crate::styled;
use crate::tree::ElementId;
use crate::view::{BuildCx, Div, View, div, send};
use rouge_reactive::{effect, signal};
use rouge_render::{Color, ImageData, ImageQuad, Point, Quad, Rect, Size};
use rouge_text::{TextLayout, TextStyle};

pub(crate) enum PaintOp {
    Quad(Quad),
    Image(ImageQuad),
    Path {
        path: Path,
        stroke: Option<(f32, bool)>,
        color: Color,
    },
    Text {
        content: String,
        style: TextStyle,
        origin: Point,
        color: Color,
        layout: Option<Box<TextLayout>>,
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
        let (content, style) = (content.into(), style.clone());
        self.ops.push(PaintOp::Text {
            content,
            style,
            origin,
            color,
            layout: None,
        });
    }
}

pub struct Canvas {
    div: Div,
    draw: Box<dyn Fn(&mut Painter)>,
}

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
            let mut painter = Painter {
                size: size.get(),
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
