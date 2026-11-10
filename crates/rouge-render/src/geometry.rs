use std::ops::{Add, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }

    pub const fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f32, height: f32) -> Self {
        Size { width, height }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const ZERO: Rect = Rect::new(0.0, 0.0, 0.0, 0.0);
    pub const EVERYTHING: Rect = Rect::new(-1.0e7, -1.0e7, 2.0e7, 2.0e7);

    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Rect {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    pub fn from_min_max(min: Point, max: Point) -> Self {
        Rect::new(
            min.x,
            min.y,
            (max.x - min.x).max(0.0),
            (max.y - min.y).max(0.0),
        )
    }

    pub fn max(&self) -> Point {
        Point::new(
            self.origin.x + self.size.width,
            self.origin.y + self.size.height,
        )
    }

    pub fn center(&self) -> Point {
        Point::new(
            self.origin.x + self.size.width * 0.5,
            self.origin.y + self.size.height * 0.5,
        )
    }

    pub fn is_empty(&self) -> bool {
        self.size.width <= 0.0 || self.size.height <= 0.0
    }

    pub fn contains(&self, point: Point) -> bool {
        let max = self.max();
        point.x >= self.origin.x && point.y >= self.origin.y && point.x < max.x && point.y < max.y
    }

    pub fn intersect(&self, other: &Rect) -> Rect {
        let (a, b) = (self.max(), other.max());
        Rect::from_min_max(
            Point::new(
                self.origin.x.max(other.origin.x),
                self.origin.y.max(other.origin.y),
            ),
            Point::new(a.x.min(b.x), a.y.min(b.y)),
        )
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let (a, b) = (self.max(), other.max());
        Rect::from_min_max(
            Point::new(
                self.origin.x.min(other.origin.x),
                self.origin.y.min(other.origin.y),
            ),
            Point::new(a.x.max(b.x), a.y.max(b.y)),
        )
    }

    pub fn translate(&self, by: Point) -> Rect {
        Rect {
            origin: self.origin + by,
            size: self.size,
        }
    }

    pub fn inflate(&self, amount: f32) -> Rect {
        Rect::new(
            self.origin.x - amount,
            self.origin.y - amount,
            (self.size.width + amount * 2.0).max(0.0),
            (self.size.height + amount * 2.0).max(0.0),
        )
    }

    pub fn to_array(self) -> [f32; 4] {
        [
            self.origin.x,
            self.origin.y,
            self.size.width,
            self.size.height,
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl Corners {
    pub const ZERO: Corners = Corners::all(0.0);

    pub const fn all(value: f32) -> Self {
        Corners {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }

    pub fn map(&self, f: impl Fn(f32) -> f32) -> Corners {
        Corners {
            top_left: f(self.top_left),
            top_right: f(self.top_right),
            bottom_right: f(self.bottom_right),
            bottom_left: f(self.bottom_left),
        }
    }

    pub fn to_array(self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }
}

impl From<f32> for Corners {
    fn from(radius: f32) -> Self {
        Corners::all(radius)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub const ZERO: Edges = Edges::all(0.0);

    pub const fn all(value: f32) -> Self {
        Edges {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn is_zero(&self) -> bool {
        *self == Edges::ZERO
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.top, self.right, self.bottom, self.left]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Affine {
    fn default() -> Self {
        Affine::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub const fn translate(x: f32, y: f32) -> Self {
        Affine {
            tx: x,
            ty: y,
            ..Affine::IDENTITY
        }
    }

    pub const fn scale(x: f32, y: f32) -> Self {
        Affine {
            a: x,
            d: y,
            ..Affine::IDENTITY
        }
    }

    pub fn rotate(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Affine {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            tx: 0.0,
            ty: 0.0,
        }
    }

    pub fn then(&self, next: &Affine) -> Affine {
        Affine {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            tx: next.a * self.tx + next.c * self.ty + next.tx,
            ty: next.b * self.tx + next.d * self.ty + next.ty,
        }
    }

    pub fn about(&self, center: Point) -> Affine {
        Affine::translate(-center.x, -center.y)
            .then(self)
            .then(&Affine::translate(center.x, center.y))
    }

    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.tx,
            self.b * p.x + self.d * p.y + self.ty,
        )
    }

    pub fn apply_rect(&self, rect: &Rect) -> Rect {
        let (min, max) = (rect.origin, rect.max());
        let corners =
            [min, Point::new(max.x, min.y), max, Point::new(min.x, max.y)].map(|p| self.apply(p));
        let fold = |f: fn(f32, f32) -> f32, start: f32| {
            corners.iter().fold(Point::new(start, start), |acc, p| {
                Point::new(f(acc.x, p.x), f(acc.y, p.y))
            })
        };
        Rect::from_min_max(
            fold(f32::min, f32::INFINITY),
            fold(f32::max, f32::NEG_INFINITY),
        )
    }

    pub fn inverse(&self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-9 {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Affine {
            a,
            b,
            c,
            d,
            tx: -(a * self.tx + c * self.ty),
            ty: -(b * self.tx + d * self.ty),
        })
    }
}
