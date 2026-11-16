use rouge_render::{Color, Corners};

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

    pub(crate) fn length(&self) -> f32 {
        self.duration
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
    Spring(Spring, SpringCurve),
    Tween(Tween),
    Decay(Decay),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Animated {
    from: f32,
    velocity: f32,
    target: f32,
    start: f64,
    end: f64,
    drive: Option<Drive>,
}

const MIN_DECAY_RATE: f32 = 1.0e-3;
const MIN_REST: f64 = 1.0e-6;
const REST_DISTANCE: f64 = 0.001;
const REST_VELOCITY: f64 = 0.01;
const CRITICAL: f64 = 1.0e-6;
const LONGEST: f64 = 3600.0;

impl Animated {
    pub(crate) fn new(value: f32) -> Self {
        Animated {
            from: value,
            velocity: 0.0,
            target: value,
            start: 0.0,
            end: 0.0,
            drive: None,
        }
    }

    pub(crate) fn value(&self, at: f64) -> f32 {
        self.state(at).0
    }

    pub(crate) fn state(&self, at: f64) -> (f32, f32) {
        let Some(drive) = self.drive else {
            return (self.target, 0.0);
        };
        if at >= self.end {
            return (self.target, 0.0);
        }
        let elapsed = (at - self.start).max(0.0);
        match drive {
            Drive::Spring(_, curve) => {
                let (x, v) = curve.state(elapsed);
                (self.target + x as f32, v as f32)
            }
            Drive::Tween(tween) => {
                let progress = if tween.duration <= 0.0 {
                    1.0
                } else {
                    (elapsed / f64::from(tween.duration)) as f32
                };
                let value = self.from + (self.target - self.from) * tween.ease.apply(progress);
                (value, 0.0)
            }
            Drive::Decay(decay) => {
                let (x, v) = decay_state(decay, self.velocity, elapsed);
                (self.from + x as f32, v as f32)
            }
        }
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

    pub(crate) fn start(&self) -> f64 {
        self.start
    }

    pub(crate) fn end(&self) -> Option<f64> {
        self.drive.map(|_| self.end)
    }

    pub(crate) fn settle(&mut self, at: f64) -> bool {
        if self.drive.is_some() && at >= self.end {
            *self = Animated::new(self.target);
        }
        self.drive.is_some()
    }

    pub(crate) fn shift(&mut self, delta: f32, at: f64) {
        self.rebase(at, |value, velocity| (value + delta, velocity));
    }

    pub(crate) fn stretch(&mut self, factor: f32, at: f64) {
        self.rebase(at, |value, velocity| (value * factor, velocity * factor));
    }

    fn rebase(&mut self, at: f64, f: impl Fn(f32, f32) -> (f32, f32)) {
        let (value, velocity) = self.state(at);
        let (value, velocity) = f(value, velocity);
        match self.drive {
            None => *self = Animated::new(value),
            Some(Drive::Tween(_)) => {
                self.from = f(self.from, 0.0).0;
                self.target = f(self.target, 0.0).0;
            }
            Some(Drive::Decay(decay)) => self.coast(value, velocity, decay, at),
            Some(Drive::Spring(spring, _)) => {
                self.begin(value, velocity, self.target, spring.into(), at);
            }
        }
    }

    pub(crate) fn animate_to(&mut self, target: f32, timing: Timing, at: f64) {
        let (value, velocity) = self.state(at);
        if target == value && self.drive.is_none() {
            return;
        }
        let velocity = match timing {
            Timing::Spring(_) => velocity,
            Timing::Tween(_) => 0.0,
        };
        self.begin(value, velocity, target, timing, at);
    }

    pub(crate) fn fling(&mut self, velocity: f32, target: f32, spring: Spring, at: f64) {
        let value = self.value(at);
        self.begin(value, velocity, target, spring.into(), at);
    }

    pub(crate) fn decay(&mut self, velocity: f32, decay: Decay, at: f64) {
        let value = self.value(at);
        *self = Animated::new(value);
        if velocity.abs() > decay.rest {
            self.coast(value, velocity, decay, at);
        }
    }

    pub(crate) fn crossing(&self, limit: f32) -> Option<f64> {
        let Some(Drive::Decay(decay)) = self.drive else {
            return None;
        };
        let rate = f64::from(decay.rate.max(MIN_DECAY_RATE));
        let (distance, velocity) = (f64::from(limit - self.from), f64::from(self.velocity));
        if distance * velocity <= 0.0 {
            return (distance == 0.0).then_some(self.start);
        }
        let remaining = 1.0 - distance * rate / velocity;
        if remaining <= 0.0 {
            return None;
        }
        let at = self.start - remaining.ln() / rate;
        (at < self.end).then_some(at)
    }

    fn begin(&mut self, from: f32, velocity: f32, target: f32, timing: Timing, at: f64) {
        let (duration, drive) = match timing {
            Timing::Spring(spring) => {
                let curve = SpringCurve::new(spring, from - target, velocity);
                (curve.settle(), Drive::Spring(spring, curve))
            }
            Timing::Tween(tween) => (f64::from(tween.duration.max(0.0)), Drive::Tween(tween)),
        };
        *self = Animated::running(from, velocity, target, drive, at, duration);
    }

    fn coast(&mut self, from: f32, velocity: f32, decay: Decay, at: f64) {
        let rest = f64::from(decay.rest).max(MIN_REST);
        let rate = f64::from(decay.rate.max(MIN_DECAY_RATE));
        let duration = ((f64::from(velocity.abs()) / rest).ln() / rate).clamp(0.0, LONGEST);
        let target = from + decay_state(decay, velocity, duration).0 as f32;
        *self = Animated::running(from, velocity, target, Drive::Decay(decay), at, duration);
    }

    fn running(
        from: f32,
        velocity: f32,
        target: f32,
        drive: Drive,
        at: f64,
        duration: f64,
    ) -> Self {
        if duration <= 0.0 {
            return Animated::new(target);
        }
        Animated {
            from,
            velocity,
            target,
            start: at,
            end: at + duration,
            drive: Some(drive),
        }
    }
}

fn fast_exp(x: f64) -> f64 {
    f64::from((x as f32).exp())
}

fn decay_state(decay: Decay, velocity: f32, elapsed: f64) -> (f64, f64) {
    let rate = f64::from(decay.rate.max(MIN_DECAY_RATE));
    let velocity = f64::from(velocity);
    let slowed = (-rate * elapsed).exp();
    (velocity / rate * (1.0 - slowed), velocity * slowed)
}

#[derive(Clone, Copy, Debug)]
enum SpringCurve {
    Under {
        x0: f64,
        v0: f64,
        decay: f64,
        frequency: f64,
        sine: f64,
    },
    Critical {
        x0: f64,
        v0: f64,
        omega: f64,
        slope: f64,
    },
    Over {
        slow: (f64, f64),
        fast: (f64, f64),
    },
}

impl SpringCurve {
    fn new(spring: Spring, displacement: f32, velocity: f32) -> Self {
        let (x0, v0) = (f64::from(displacement), f64::from(velocity));
        let mass = f64::from(spring.mass).max(1.0e-6);
        let stiffness = f64::from(spring.stiffness).max(1.0e-6);
        let damping = f64::from(spring.damping).max(0.0);
        let omega = (stiffness / mass).sqrt();
        let zeta = damping / (2.0 * (stiffness * mass).sqrt());
        if (zeta - 1.0).abs() <= CRITICAL {
            SpringCurve::Critical {
                x0,
                v0,
                omega,
                slope: v0 + omega * x0,
            }
        } else if zeta < 1.0 {
            let decay = zeta * omega;
            let frequency = omega * (1.0 - zeta * zeta).sqrt();
            SpringCurve::Under {
                x0,
                v0,
                decay,
                frequency,
                sine: (v0 + decay * x0) / frequency,
            }
        } else {
            let spread = omega * (zeta * zeta - 1.0).sqrt();
            let (slow, fast) = (-zeta * omega + spread, -zeta * omega - spread);
            let c_fast = (v0 - slow * x0) / (fast - slow);
            SpringCurve::Over {
                slow: (x0 - c_fast, slow),
                fast: (c_fast, fast),
            }
        }
    }

    fn state(&self, t: f64) -> (f64, f64) {
        match *self {
            SpringCurve::Under {
                x0,
                v0,
                decay,
                frequency,
                sine,
            } => {
                let envelope = fast_exp(-decay * t);
                let (sin, cos) = ((frequency * t) as f32).sin_cos();
                let (sin, cos) = (f64::from(sin), f64::from(cos));
                (
                    envelope * (x0 * cos + sine * sin),
                    envelope * (v0 * cos - (x0 * frequency + decay * sine) * sin),
                )
            }
            SpringCurve::Critical {
                x0,
                v0,
                omega,
                slope,
            } => {
                let envelope = fast_exp(-omega * t);
                (
                    envelope * (x0 + slope * t),
                    envelope * (v0 - omega * slope * t),
                )
            }
            SpringCurve::Over {
                slow: (c1, r1),
                fast: (c2, r2),
            } => {
                let (e1, e2) = (fast_exp(r1 * t), fast_exp(r2 * t));
                (c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
            }
        }
    }

    fn settle(&self) -> f64 {
        let duration = match *self {
            SpringCurve::Under {
                x0,
                v0,
                decay,
                frequency,
                sine,
            } => {
                let distance = x0.hypot(sine);
                let speed = v0.hypot(x0 * frequency + decay * sine);
                let within = |amplitude: f64, rest: f64| {
                    if amplitude <= rest {
                        0.0
                    } else {
                        (amplitude / rest).ln() / decay
                    }
                };
                within(distance, REST_DISTANCE).max(within(speed, REST_VELOCITY))
            }
            SpringCurve::Critical {
                x0,
                v0,
                omega,
                slope,
            } => {
                let distance = Bound::linear(x0.abs(), slope.abs(), omega);
                let speed = Bound::linear(v0.abs(), (omega * slope).abs(), omega);
                distance
                    .below(REST_DISTANCE)
                    .max(speed.below(REST_VELOCITY))
            }
            SpringCurve::Over {
                slow: (c1, r1),
                fast: (c2, r2),
            } => {
                let distance = Bound::exponential([(c1.abs(), r1), (c2.abs(), r2)]);
                let speed = Bound::exponential([((c1 * r1).abs(), r1), ((c2 * r2).abs(), r2)]);
                distance
                    .below(REST_DISTANCE)
                    .max(speed.below(REST_VELOCITY))
            }
        };
        duration.clamp(0.0, LONGEST)
    }
}

enum Bound {
    Linear { base: f64, slope: f64, rate: f64 },
    Exponential([(f64, f64); 2]),
}

impl Bound {
    fn linear(base: f64, slope: f64, rate: f64) -> Self {
        Bound::Linear { base, slope, rate }
    }

    fn exponential(terms: [(f64, f64); 2]) -> Self {
        Bound::Exponential(terms)
    }

    fn at(&self, t: f64) -> f64 {
        match *self {
            Bound::Linear { base, slope, rate } => (base + slope * t) * (-rate * t).exp(),
            Bound::Exponential(terms) => terms.iter().map(|(c, r)| c * (r * t).exp()).sum(),
        }
    }

    fn peak(&self) -> f64 {
        match *self {
            Bound::Linear { base, slope, rate } if slope > 0.0 => {
                (1.0 / rate - base / slope).max(0.0)
            }
            _ => 0.0,
        }
    }

    fn below(&self, rest: f64) -> f64 {
        let low = self.peak();
        if self.at(low) < rest {
            return low;
        }
        let (mut low, mut high) = (low, low + 0.01);
        while self.at(high) >= rest {
            if high >= LONGEST {
                return LONGEST;
            }
            (low, high) = (high, high * 2.0);
        }
        for _ in 0..48 {
            let middle = f64::midpoint(low, high);
            if self.at(middle) < rest {
                high = middle;
            } else {
                low = middle;
            }
        }
        high
    }
}
