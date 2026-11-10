use crate::{Color, Corners, Point, Rect};
use std::any::TypeId;
use std::hash::{Hash, Hasher};

const SHADOW_REACH: f32 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inputs(u8);

impl Inputs {
    pub const CONTENT: Inputs = Inputs(1);
    pub const BACKDROP: Inputs = Inputs(2);
    pub const BOTH: Inputs = Inputs(3);

    pub const fn content(self) -> bool {
        self.0 & 1 != 0
    }

    pub const fn backdrop(self) -> bool {
        self.0 & 2 != 0
    }
}

pub trait Effect: 'static {
    const WGSL: &'static str;
    const INPUTS: Inputs;

    fn uniforms(&self) -> Vec<f32> {
        Vec::new()
    }

    fn margin(&self) -> f32 {
        0.0
    }

    fn content_blur(&self) -> f32 {
        0.0
    }

    fn backdrop_blur(&self) -> f32 {
        0.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Shader {
    id: TypeId,
    wgsl: &'static str,
    inputs: Inputs,
}

impl Shader {
    pub fn of<T: Effect>() -> Self {
        Shader {
            id: TypeId::of::<T>(),
            wgsl: T::WGSL,
            inputs: T::INPUTS,
        }
    }

    pub fn wgsl(&self) -> &'static str {
        self.wgsl
    }

    pub fn inputs(&self) -> Inputs {
        self.inputs
    }
}

impl PartialEq for Shader {
    fn eq(&self, other: &Shader) -> bool {
        self.id == other.id
    }
}

impl Eq for Shader {}

impl Hash for Shader {
    fn hash<T: Hasher>(&self, state: &mut T) {
        self.id.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectInstance {
    shader: Shader,
    values: [f32; EffectInstance::VALUES],
}

impl EffectInstance {
    pub const UNIFORMS: usize = 16;
    pub const VALUES: usize = Self::UNIFORMS + 3;

    pub fn new<T: Effect>(effect: &T) -> Self {
        let uniforms = effect.uniforms();
        assert!(
            uniforms.len() <= Self::UNIFORMS,
            "rouge: an effect has at most {} uniform values",
            Self::UNIFORMS
        );
        let mut values = [0.0; Self::VALUES];
        values[0] = effect.margin().max(0.0);
        values[1] = effect.content_blur().max(0.0);
        values[2] = effect.backdrop_blur().max(0.0);
        values[3..3 + uniforms.len()].copy_from_slice(&uniforms);
        EffectInstance {
            shader: Shader::of::<T>(),
            values,
        }
    }

    pub fn from_values(shader: Shader, values: [f32; Self::VALUES]) -> Self {
        EffectInstance { shader, values }
    }

    pub fn shader(&self) -> Shader {
        self.shader
    }

    pub fn values(&self) -> &[f32; Self::VALUES] {
        &self.values
    }

    pub fn margin(&self) -> f32 {
        self.values[0]
    }

    pub fn content_blur(&self) -> f32 {
        self.values[1]
    }

    pub fn backdrop_blur(&self) -> f32 {
        self.values[2]
    }

    pub fn uniforms(&self) -> &[f32] {
        &self.values[3..]
    }
}

impl<T: Effect> From<&T> for EffectInstance {
    fn from(effect: &T) -> Self {
        EffectInstance::new(effect)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectDraw {
    pub effect: EffectInstance,
    pub bounds: Rect,
    pub corner_radii: Corners,
}

impl EffectDraw {
    pub fn new<T: Effect>(effect: &T, bounds: Rect) -> Self {
        EffectDraw {
            effect: EffectInstance::new(effect),
            bounds,
            corner_radii: Corners::ZERO,
        }
    }
}

pub(crate) struct Passthrough;

impl Effect for Passthrough {
    const WGSL: &'static str = include_str!("shaders/passthrough.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blur {
    pub radius: f32,
}

impl Blur {
    pub const fn new(radius: f32) -> Self {
        Blur { radius }
    }
}

impl Effect for Blur {
    const WGSL: &'static str = include_str!("shaders/passthrough.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;

    fn content_blur(&self) -> f32 {
        self.radius
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropBlur {
    pub radius: f32,
}

impl BackdropBlur {
    pub const fn new(radius: f32) -> Self {
        BackdropBlur { radius }
    }
}

impl Effect for BackdropBlur {
    const WGSL: &'static str = include_str!("shaders/backdrop_blur.wgsl");
    const INPUTS: Inputs = Inputs::BACKDROP;

    fn margin(&self) -> f32 {
        1.0
    }

    fn backdrop_blur(&self) -> f32 {
        self.radius
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorMatrix {
    pub rows: [[f32; 4]; 3],
}

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

impl ColorMatrix {
    pub const IDENTITY: ColorMatrix = ColorMatrix {
        rows: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
    };

    pub const fn new(rows: [[f32; 4]; 3]) -> Self {
        ColorMatrix { rows }
    }

    pub fn saturate(amount: f32) -> Self {
        let mut rows = [[0.0; 4]; 3];
        for (row, cells) in rows.iter_mut().enumerate() {
            for (column, luma) in LUMA.into_iter().enumerate() {
                cells[column] = luma * (1.0 - amount) + if row == column { amount } else { 0.0 };
            }
        }
        ColorMatrix { rows }
    }

    pub fn grayscale() -> Self {
        Self::saturate(0.0)
    }

    pub fn brightness(amount: f32) -> Self {
        Self::scale(amount, 0.0)
    }

    pub fn contrast(amount: f32) -> Self {
        Self::scale(amount, 0.5 - 0.5 * amount)
    }

    pub fn invert() -> Self {
        Self::scale(-1.0, 1.0)
    }

    pub fn tint(color: Color) -> Self {
        let mut matrix = Self::IDENTITY;
        for (cells, channel) in matrix.rows.iter_mut().zip([color.r, color.g, color.b]) {
            for cell in cells.iter_mut().take(3) {
                *cell *= 1.0 - color.a;
            }
            cells[3] = channel * color.a;
        }
        matrix
    }

    pub fn then(&self, next: &ColorMatrix) -> ColorMatrix {
        let mut rows = [[0.0; 4]; 3];
        for (out, by) in rows.iter_mut().zip(next.rows) {
            for (column, cell) in out.iter_mut().enumerate() {
                let through: f32 = (0..3).map(|k| by[k] * self.rows[k][column]).sum();
                *cell = through + if column == 3 { by[3] } else { 0.0 };
            }
        }
        ColorMatrix { rows }
    }

    fn scale(factor: f32, offset: f32) -> Self {
        let mut matrix = Self::IDENTITY;
        for (index, cells) in matrix.rows.iter_mut().enumerate() {
            cells[index] = factor;
            cells[3] = offset;
        }
        matrix
    }
}

impl Effect for ColorMatrix {
    const WGSL: &'static str = include_str!("shaders/color_matrix.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;

    fn uniforms(&self) -> Vec<f32> {
        self.rows.as_flattened().to_vec()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MaskKind {
    Linear { from: Point, to: Point },
    Radial { center: Point, radius: f32 },
    Shape,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mask {
    kind: MaskKind,
    stops: (f32, f32),
    inverted: bool,
}

impl Mask {
    const fn of(kind: MaskKind) -> Self {
        Mask {
            kind,
            stops: (0.0, 1.0),
            inverted: false,
        }
    }

    pub const fn linear_gradient(from: Point, to: Point) -> Self {
        Mask::of(MaskKind::Linear { from, to })
    }

    pub const fn radial_gradient(center: Point, radius: f32) -> Self {
        Mask::of(MaskKind::Radial { center, radius })
    }

    pub const fn shape() -> Self {
        Mask::of(MaskKind::Shape)
    }

    pub const fn stops(mut self, opaque_until: f32, clear_from: f32) -> Self {
        self.stops = (opaque_until, clear_from);
        self
    }

    pub const fn invert(mut self) -> Self {
        self.inverted = true;
        self
    }
}

impl Effect for Mask {
    const WGSL: &'static str = include_str!("shaders/mask.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;

    fn uniforms(&self) -> Vec<f32> {
        let (kind, shape) = match self.kind {
            MaskKind::Linear { from, to } => (0.0, [from.x, from.y, to.x, to.y]),
            MaskKind::Radial { center, radius } => (1.0, [center.x, center.y, radius, 0.0]),
            MaskKind::Shape => (2.0, [0.0; 4]),
        };
        let mut uniforms = vec![
            kind,
            f32::from(u8::from(self.inverted)),
            self.stops.0,
            self.stops.1,
        ];
        uniforms.extend(shape);
        uniforms
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    PlusLighter,
}

impl Effect for BlendMode {
    const WGSL: &'static str = include_str!("shaders/blend.wgsl");
    const INPUTS: Inputs = Inputs::BOTH;

    fn uniforms(&self) -> Vec<f32> {
        vec![f32::from(*self as u8)]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glass {
    blur: f32,
    tint: Color,
    refraction: f32,
    dispersion: f32,
    highlight: f32,
    edge_width: f32,
    shadow: f32,
}

impl Default for Glass {
    fn default() -> Self {
        Self::new()
    }
}

impl Glass {
    pub const fn new() -> Self {
        Glass {
            blur: 10.0,
            tint: Color::new(1.0, 1.0, 1.0, 0.08),
            refraction: 0.7,
            dispersion: 0.12,
            highlight: 0.35,
            edge_width: 14.0,
            shadow: 0.25,
        }
    }

    pub const fn frosted(blur: f32) -> Self {
        Glass {
            blur,
            tint: Color::TRANSPARENT,
            refraction: 0.0,
            dispersion: 0.0,
            highlight: 0.0,
            edge_width: 1.0,
            shadow: 0.0,
        }
    }

    pub const fn blur(mut self, radius: f32) -> Self {
        self.blur = radius;
        self
    }

    pub const fn tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }

    pub const fn refraction(mut self, amount: f32) -> Self {
        self.refraction = amount;
        self
    }

    pub const fn dispersion(mut self, amount: f32) -> Self {
        self.dispersion = amount;
        self
    }

    pub const fn highlight(mut self, amount: f32) -> Self {
        self.highlight = amount;
        self
    }

    pub const fn edge_width(mut self, width: f32) -> Self {
        self.edge_width = width;
        self
    }

    pub const fn shadow(mut self, amount: f32) -> Self {
        self.shadow = amount;
        self
    }
}

impl Effect for Glass {
    const WGSL: &'static str = include_str!("shaders/glass.wgsl");
    const INPUTS: Inputs = Inputs::BACKDROP;

    fn uniforms(&self) -> Vec<f32> {
        let mut uniforms = self.tint.to_array().to_vec();
        uniforms.extend([
            self.refraction,
            self.dispersion,
            self.highlight,
            self.edge_width,
            self.shadow,
        ]);
        uniforms
    }

    fn margin(&self) -> f32 {
        let outward =
            (-self.refraction).max(0.0) * self.edge_width * 2.0 * (1.0 + self.dispersion.abs());
        let shadow = if self.shadow > 0.0 { SHADOW_REACH } else { 1.0 };
        shadow.max(outward)
    }

    fn backdrop_blur(&self) -> f32 {
        self.blur
    }
}
