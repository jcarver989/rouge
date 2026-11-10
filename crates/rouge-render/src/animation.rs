use crate::{Color, Corners};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

impl Spring {
    pub const fn new(stiffness: f32, damping: f32) -> Self {
        Spring {
            stiffness,
            damping,
            mass: 1.0,
        }
    }

    pub const fn snappy() -> Self {
        Spring::new(400.0, 40.0)
    }

    pub const fn smooth() -> Self {
        Spring::new(170.0, 26.0)
    }

    pub const fn bouncy() -> Self {
        Spring::new(300.0, 14.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Ease {
    Linear,
    In,
    #[default]
    Out,
    InOut,
    Bezier(f32, f32, f32, f32),
}

impl Ease {
    pub const fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Ease::Bezier(x1.clamp(0.0, 1.0), y1, x2.clamp(0.0, 1.0), y2)
    }

    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::In => t * t * t,
            Ease::Out => 1.0 - (1.0 - t).powi(3),
            Ease::InOut if t < 0.5 => 4.0 * t * t * t,
            Ease::InOut => 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0,
            Ease::Bezier(x1, y1, x2, y2) => {
                let curve = |a: f32, b: f32, s: f32| {
                    let r = 1.0 - s;
                    3.0 * r * r * s * a + 3.0 * r * s * s * b + s * s * s
                };
                let (mut low, mut high, mut s) = (0.0, 1.0, t);
                for _ in 0..24 {
                    let x = curve(x1, x2, s);
                    if (x - t).abs() < 1e-6 {
                        break;
                    }
                    if x < t {
                        low = s;
                    } else {
                        high = s;
                    }
                    s = f32::midpoint(low, high);
                }
                curve(y1, y2, s)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub(crate) duration: f32,
    pub(crate) ease: Ease,
}

impl Tween {
    pub const fn new(seconds: f32) -> Self {
        Tween {
            duration: seconds,
            ease: Ease::Out,
        }
    }

    pub const fn ms(milliseconds: f32) -> Self {
        Tween::new(milliseconds / 1000.0)
    }

    pub const fn ease(mut self, ease: Ease) -> Self {
        self.ease = ease;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Timing {
    Spring(Spring),
    Tween(Tween),
}

impl From<Spring> for Timing {
    fn from(spring: Spring) -> Self {
        Timing::Spring(spring)
    }
}

impl From<Tween> for Timing {
    fn from(tween: Tween) -> Self {
        Timing::Tween(tween)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decay {
    pub(crate) rate: f32,
    pub(crate) rest: f32,
}

impl Default for Decay {
    fn default() -> Self {
        Self::new()
    }
}

impl Decay {
    pub const fn new() -> Self {
        Decay {
            rate: 2.0,
            rest: 1.0,
        }
    }

    pub const fn rate(mut self, rate: f32) -> Self {
        self.rate = rate;
        self
    }

    pub const fn rest(mut self, velocity: f32) -> Self {
        self.rest = velocity;
        self
    }

    pub fn distance(&self, velocity: f32) -> f32 {
        velocity / self.rate.max(MIN_DECAY_RATE)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutTransition {
    pub(crate) timing: Timing,
    pub(crate) size: bool,
}

impl LayoutTransition {
    pub fn new(timing: impl Into<Timing>) -> Self {
        LayoutTransition {
            timing: timing.into(),
            size: false,
        }
    }

    pub const fn size(mut self) -> Self {
        self.size = true;
        self
    }
}

impl<T: Into<Timing>> From<T> for LayoutTransition {
    fn from(timing: T) -> Self {
        LayoutTransition::new(timing)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayerProp {
    TranslateX,
    TranslateY,
    Scale,
    Rotate,
    Opacity,
    FillRed,
    FillGreen,
    FillBlue,
    FillAlpha,
    RadiusTopLeft,
    RadiusTopRight,
    RadiusBottomRight,
    RadiusBottomLeft,
}

impl LayerProp {
    pub const ALL: [LayerProp; 13] = [
        LayerProp::TranslateX,
        LayerProp::TranslateY,
        LayerProp::Scale,
        LayerProp::Rotate,
        LayerProp::Opacity,
        LayerProp::FillRed,
        LayerProp::FillGreen,
        LayerProp::FillBlue,
        LayerProp::FillAlpha,
        LayerProp::RadiusTopLeft,
        LayerProp::RadiusTopRight,
        LayerProp::RadiusBottomRight,
        LayerProp::RadiusBottomLeft,
    ];

    pub const FILL: [LayerProp; 4] = [
        LayerProp::FillRed,
        LayerProp::FillGreen,
        LayerProp::FillBlue,
        LayerProp::FillAlpha,
    ];

    pub const RADII: [LayerProp; 4] = [
        LayerProp::RadiusTopLeft,
        LayerProp::RadiusTopRight,
        LayerProp::RadiusBottomRight,
        LayerProp::RadiusBottomLeft,
    ];

    pub const fn initial(self) -> f32 {
        match self {
            LayerProp::Scale | LayerProp::Opacity => 1.0,
            _ => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    values: [Option<f32>; LayerProp::ALL.len()],
}

impl Pose {
    pub const fn new() -> Self {
        Pose {
            values: [None; LayerProp::ALL.len()],
        }
    }

    pub const fn with(mut self, prop: LayerProp, value: f32) -> Self {
        self.values[prop as usize] = Some(value);
        self
    }

    pub const fn translate_x(self, value: f32) -> Self {
        self.with(LayerProp::TranslateX, value)
    }

    pub const fn translate_y(self, value: f32) -> Self {
        self.with(LayerProp::TranslateY, value)
    }

    pub const fn scale(self, value: f32) -> Self {
        self.with(LayerProp::Scale, value)
    }

    pub const fn rotate(self, degrees: f32) -> Self {
        self.with(LayerProp::Rotate, degrees)
    }

    pub const fn opacity(self, value: f32) -> Self {
        self.with(LayerProp::Opacity, value)
    }

    pub fn background(mut self, color: Color) -> Self {
        for (prop, value) in LayerProp::FILL.into_iter().zip(color.premultiplied()) {
            self = self.with(prop, value);
        }
        self
    }

    pub fn rounded(mut self, radii: impl Into<Corners>) -> Self {
        for (prop, radius) in LayerProp::RADII.into_iter().zip(radii.into().to_array()) {
            self = self.with(prop, radius);
        }
        self
    }

    pub fn get(&self, prop: LayerProp) -> Option<f32> {
        self.values[prop as usize]
    }

    pub fn iter(&self) -> impl Iterator<Item = (LayerProp, f32)> + '_ {
        LayerProp::ALL
            .into_iter()
            .filter_map(|prop| Some((prop, self.get(prop)?)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Keyframes {
    stops: Vec<(f32, Pose)>,
    duration: f32,
    ease: Ease,
    repeat: bool,
}

impl Default for Keyframes {
    fn default() -> Self {
        Self::new()
    }
}

impl Keyframes {
    pub const fn new() -> Self {
        Keyframes {
            stops: Vec::new(),
            duration: 1.0,
            ease: Ease::Linear,
            repeat: false,
        }
    }

    pub fn at(mut self, offset: f32, pose: impl FnOnce(Pose) -> Pose) -> Self {
        let offset = offset.clamp(0.0, 1.0);
        let index = self.stops.partition_point(|(at, _)| *at <= offset);
        self.stops.insert(index, (offset, pose(Pose::new())));
        self
    }

    pub const fn duration(mut self, seconds: f32) -> Self {
        self.duration = seconds;
        self
    }

    pub const fn ease(mut self, ease: Ease) -> Self {
        self.ease = ease;
        self
    }

    pub const fn repeat(mut self) -> Self {
        self.repeat = true;
        self
    }

    pub fn is_finished(&self, elapsed: f32) -> bool {
        !self.repeat && elapsed >= self.duration
    }

    pub fn progress(&self, elapsed: f32) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        let progress = elapsed / self.duration;
        if self.repeat {
            progress.fract()
        } else {
            progress.min(1.0)
        }
    }

    pub fn sample(&self, prop: LayerProp, progress: f32) -> Option<f32> {
        let mut stops = self
            .stops
            .iter()
            .filter_map(|(at, pose)| Some((*at, pose.get(prop)?)));
        let mut previous = stops.next()?;
        if progress <= previous.0 {
            return Some(previous.1);
        }
        for next in stops {
            if progress < next.0 {
                let t = (progress - previous.0) / (next.0 - previous.0);
                return Some(previous.1 + (next.1 - previous.1) * self.ease.apply(t));
            }
            previous = next;
        }
        Some(previous.1)
    }
}

#[derive(Clone, Copy, Debug)]
enum Drive {
    Spring(Spring),
    Tween(Tween),
    Decay(Decay),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Animated {
    value: f32,
    velocity: f32,
    target: f32,
    from: f32,
    elapsed: f32,
    drive: Option<Drive>,
}

const MAX_STEP: f32 = 1.0 / 240.0;
const MIN_DECAY_RATE: f32 = 1.0e-3;
pub(crate) const MAX_TICK: f32 = 1.0 / 15.0;

impl Animated {
    pub(crate) fn new(value: f32) -> Self {
        Animated {
            value,
            velocity: 0.0,
            target: value,
            from: value,
            elapsed: 0.0,
            drive: None,
        }
    }

    pub(crate) fn value(&self) -> f32 {
        self.value
    }

    pub(crate) fn target(&self) -> f32 {
        self.target
    }

    pub(crate) fn is_animating(&self) -> bool {
        self.drive.is_some()
    }

    pub(crate) fn is_decaying(&self) -> bool {
        matches!(self.drive, Some(Drive::Decay(_)))
    }

    pub(crate) fn shift(&mut self, delta: f32) {
        self.value += delta;
        self.from += delta;
    }

    pub(crate) fn stretch(&mut self, factor: f32) {
        self.value *= factor;
        self.from *= factor;
        self.velocity *= factor;
    }

    pub(crate) fn animate_to(&mut self, target: f32, timing: Timing) {
        if target == self.value && self.drive.is_none() {
            return;
        }
        let drive = match timing {
            Timing::Spring(spring) => Drive::Spring(spring),
            Timing::Tween(tween) => {
                *self = Animated::new(self.value);
                Drive::Tween(tween)
            }
        };
        self.target = target;
        self.drive = Some(drive);
    }

    pub(crate) fn fling(&mut self, velocity: f32, target: f32, spring: Spring) {
        self.velocity = velocity;
        self.target = target;
        self.drive = Some(Drive::Spring(spring));
    }

    pub(crate) fn decay(&mut self, velocity: f32, decay: Decay) {
        *self = Animated::new(self.value);
        if velocity.abs() > decay.rest {
            self.velocity = velocity;
            self.target = self.value + decay.distance(velocity);
            self.drive = Some(Drive::Decay(decay));
        }
    }

    pub(crate) fn tick(&mut self, dt: f32) -> bool {
        let Some(drive) = self.drive else {
            return false;
        };
        let dt = dt.clamp(0.0, MAX_TICK);
        let done = match drive {
            Drive::Spring(spring) => {
                let mut remaining = dt;
                while remaining > 0.0 {
                    let step = remaining.min(MAX_STEP);
                    let force = -spring.stiffness * (self.value - self.target)
                        - spring.damping * self.velocity;
                    self.velocity += force / spring.mass * step;
                    self.value += self.velocity * step;
                    remaining -= step;
                }
                (self.value - self.target).abs() < 0.001 && self.velocity.abs() < 0.01
            }
            Drive::Tween(tween) => {
                self.elapsed += dt;
                let progress = if tween.duration <= 0.0 {
                    1.0
                } else {
                    self.elapsed / tween.duration
                };
                self.value = self.from + (self.target - self.from) * tween.ease.apply(progress);
                progress >= 1.0
            }
            Drive::Decay(decay) => {
                let rate = decay.rate.max(MIN_DECAY_RATE);
                let slowed = self.velocity * (-rate * dt).exp();
                self.value += (self.velocity - slowed) / rate;
                self.velocity = slowed;
                self.target = self.value;
                slowed.abs() <= decay.rest
            }
        };
        if done {
            *self = Animated::new(self.target);
        }
        !done
    }
}
