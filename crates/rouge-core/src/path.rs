use rouge_render::{Affine, Point, Rect, Size};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;
use zeno::{Command, PathBuilder as _, PathData, Vector};

#[derive(Clone, Debug, Default)]
pub struct PathBuilder {
    commands: Vec<Command>,
}

impl PathBuilder {
    pub fn move_to(mut self, x: f32, y: f32) -> Self {
        self.commands.push(Command::MoveTo(Vector::new(x, y)));
        self
    }

    pub fn line_to(mut self, x: f32, y: f32) -> Self {
        self.commands.push(Command::LineTo(Vector::new(x, y)));
        self
    }

    pub fn quad_to(mut self, cx: f32, cy: f32, x: f32, y: f32) -> Self {
        self.commands
            .push(Command::QuadTo(Vector::new(cx, cy), Vector::new(x, y)));
        self
    }

    pub fn cubic_to(mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) -> Self {
        let (c1, c2) = (Vector::new(c1x, c1y), Vector::new(c2x, c2y));
        self.commands
            .push(Command::CurveTo(c1, c2, Vector::new(x, y)));
        self
    }

    pub fn close(mut self) -> Self {
        self.commands.push(Command::Close);
        self
    }

    pub fn rect(mut self, rect: Rect) -> Self {
        let (origin, size) = (rect.origin, rect.size);
        self.commands
            .add_rect((origin.x, origin.y), size.width, size.height);
        self
    }

    pub fn ellipse(mut self, center: Point, radii: Size) -> Self {
        self.commands
            .add_ellipse((center.x, center.y), radii.width, radii.height);
        self
    }

    pub fn circle(self, center: Point, radius: f32) -> Self {
        self.ellipse(center, Size::new(radius, radius))
    }

    pub fn build(self) -> Path {
        Path::from_commands(self.commands)
    }
}

#[derive(Clone, Debug)]
pub struct Path {
    commands: Rc<[Command]>,
    hash: u64,
}

impl PartialEq for Path {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.commands == other.commands
    }
}

impl Path {
    pub fn builder() -> PathBuilder {
        PathBuilder::default()
    }

    pub fn svg(data: &str) -> Path {
        Path::from_commands(data.commands().collect())
    }

    fn from_commands(commands: Vec<Command>) -> Path {
        let mut hasher = DefaultHasher::new();
        for command in &commands {
            let (verb, points): (u8, &[Vector]) = match command {
                Command::MoveTo(to) => (0, &[*to]),
                Command::LineTo(to) => (1, &[*to]),
                Command::QuadTo(control, to) => (2, &[*control, *to]),
                Command::CurveTo(first, second, to) => (3, &[*first, *second, *to]),
                Command::Close => (4, &[]),
            };
            verb.hash(&mut hasher);
            for point in points {
                (point.x.to_bits(), point.y.to_bits()).hash(&mut hasher);
            }
        }
        Path {
            hash: hasher.finish(),
            commands: commands.into(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn transform(&self, transform: Affine) -> Path {
        let map = |point: &Vector| {
            let mapped = transform.apply(Point::new(point.x, point.y));
            Vector::new(mapped.x, mapped.y)
        };
        let commands = self.commands.iter().map(|command| match command {
            Command::MoveTo(to) => Command::MoveTo(map(to)),
            Command::LineTo(to) => Command::LineTo(map(to)),
            Command::QuadTo(control, to) => Command::QuadTo(map(control), map(to)),
            Command::CurveTo(first, second, to) => {
                Command::CurveTo(map(first), map(second), map(to))
            }
            Command::Close => Command::Close,
        });
        Path::from_commands(commands.collect())
    }

    pub fn fit(&self, view_box: Size, into: Rect) -> Path {
        let scale = (into.size.width / view_box.width).min(into.size.height / view_box.height);
        let offset = Point::new(
            into.origin.x + (into.size.width - view_box.width * scale) / 2.0,
            into.origin.y + (into.size.height - view_box.height * scale) / 2.0,
        );
        self.transform(Affine {
            a: scale,
            b: 0.0,
            c: 0.0,
            d: scale,
            tx: offset.x,
            ty: offset.y,
        })
    }

    pub(crate) fn commands(&self) -> &[Command] {
        &self.commands
    }

    pub(crate) fn id(&self) -> u64 {
        self.hash
    }
}
