use crate::animation::{
    Animated, Decay, Keyframes, LayerProp, LayoutTransition, Pose, Spring, Timing,
};
use rouge_render::{Affine, Corners, EffectInstance, Point, Rect, Size, SurfaceId};
use rustc_hash::FxHashMap;
use slotmap::{SecondaryMap, SlotMap, new_key_type};
use std::collections::BTreeMap;
use std::mem::{replace, take};

const RUBBER_BAND: f32 = 0.25;
const BOUNCE: Spring = Spring::new(220.0, 30.0);
const CLOCK_PERIOD: f64 = 3600.0;

new_key_type! {
    pub struct LayerId;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MotionId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollState {
    pub offset: Point,
    pub viewport: Size,
    pub content: Size,
}

impl ScrollState {
    pub fn scroll_to(&mut self, offset: Point) {
        self.offset = offset;
    }

    pub fn max_offset(&self) -> Point {
        Point::new(
            (self.content.width - self.viewport.width).max(0.0),
            (self.content.height - self.viewport.height).max(0.0),
        )
    }

    pub fn clamp(&self, offset: Point) -> Point {
        let max = self.max_offset();
        Point::new(offset.x.clamp(0.0, max.x), offset.y.clamp(0.0, max.y))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerPose {
    pub translate: Point,
    pub scroll: Point,
    pub stretch: Size,
    pub scale: f32,
    pub rotate: f32,
    pub opacity: f32,
}

impl LayerPose {
    pub fn transform(&self, bounds: Rect) -> Affine {
        let offset = self.translate - self.scroll;
        self.shape(bounds)
            .then(&Affine::translate(offset.x, offset.y))
    }

    pub(crate) fn shape(&self, bounds: Rect) -> Affine {
        let spin =
            Affine::scale(self.scale, self.scale).then(&Affine::rotate(self.rotate.to_radians()));
        Affine::scale(self.stretch.width, self.stretch.height)
            .about(bounds.origin)
            .then(&spin.about(bounds.center()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectSlot {
    Content(usize),
    Mask,
    Blend,
    Backdrop,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nested {
    pub layer: LayerId,
    pub clip: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollLinked {
    Sticky {
        lo: f32,
        hi: f32,
    },
    Linear {
        factor: f32,
        max: f32,
        horizontal: bool,
    },
}

impl ScrollLinked {
    pub(crate) fn offset(self, scroll: Point) -> (f32, f32) {
        match self {
            ScrollLinked::Sticky { lo, hi } => (0.0, scroll.y.clamp(lo, hi.max(lo)) - scroll.y),
            ScrollLinked::Linear {
                factor,
                max,
                horizontal,
            } => {
                let along = |offset: f32| (offset * factor).clamp(0.0, max.max(0.0));
                if horizontal {
                    (along(scroll.x), 0.0)
                } else {
                    (0.0, along(scroll.y))
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Value {
    Own(Animated),
    Motion(MotionId),
}

type Motions = FxHashMap<MotionId, Animated>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Applied {
    pub(crate) effect: EffectInstance,
    values: [Animated; EffectInstance::VALUES],
    clock: f64,
}

impl Applied {
    fn new(effect: &EffectInstance, clock: f64) -> Self {
        Applied {
            effect: *effect,
            values: effect.values().map(Animated::new),
            clock,
        }
    }

    pub(crate) fn instance(&self, at: f64) -> EffectInstance {
        let elapsed = (at - self.clock).max(0.0) % CLOCK_PERIOD;
        self.effect
            .with_values(self.values.map(|value| value.value(at)))
            .at(elapsed as f32)
    }

    fn retarget(&mut self, effect: &EffectInstance, timing: Timing, at: f64) -> bool {
        let mut moving = false;
        for (value, target) in self.values.iter_mut().zip(effect.values()) {
            value.animate_to(*target, timing, at);
            moving |= value.is_animating();
        }
        moving
    }

    fn settle(&mut self, at: f64) -> bool {
        let moving = self
            .values
            .iter_mut()
            .fold(false, |moving, value| value.settle(at) | moving);
        moving || self.effect.clock().is_some()
    }

    fn end(&self) -> Option<f64> {
        if self.effect.clock().is_some() {
            return Some(f64::INFINITY);
        }
        latest(self.values.iter().map(Animated::end))
    }

    fn first_end(&self) -> Option<f64> {
        Some(earliest(self.values.iter().map(Animated::end))).filter(|end| end.is_finite())
    }
}

fn latest(ends: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    ends.into_iter().flatten().reduce(f64::max)
}

fn earliest(ends: impl IntoIterator<Item = Option<f64>>) -> f64 {
    ends.into_iter().flatten().fold(f64::INFINITY, f64::min)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Fling {
    glide: Animated,
    bounce: Option<Animated>,
}

impl Fling {
    fn rest(offset: f32) -> Self {
        Fling {
            glide: Animated::new(offset),
            bounce: None,
        }
    }

    fn release(offset: f32, velocity: f32, max: f32, at: f64) -> Self {
        let mut fling = Fling::rest(offset);
        if offset < 0.0 || offset > max {
            fling
                .glide
                .fling(velocity, offset.clamp(0.0, max), BOUNCE, at);
        } else if max > 0.0 {
            fling.glide.decay(velocity, Decay::new(), at);
            let limit = if velocity > 0.0 { max } else { 0.0 };
            fling.bounce = fling.glide.crossing(limit).map(|crossed| {
                let speed = fling.glide.state(crossed).1;
                let mut bounce = Animated::new(limit);
                bounce.fling(speed, limit, BOUNCE, crossed);
                bounce
            });
        }
        fling
    }

    fn current(&self, at: f64) -> &Animated {
        match &self.bounce {
            Some(bounce) if at >= bounce.start() => bounce,
            _ => &self.glide,
        }
    }

    fn state(&self, at: f64) -> (f32, f32) {
        self.current(at).state(at)
    }

    fn is_animating(&self) -> bool {
        self.glide.is_animating() || self.bounce.is_some()
    }

    fn is_gliding(&self, at: f64) -> bool {
        self.current(at).is_decaying()
    }

    fn settle(&mut self, at: f64) -> bool {
        if let Some(bounce) = self.bounce.filter(|bounce| at >= bounce.start()) {
            self.glide = bounce;
            self.bounce = None;
        }
        self.glide.settle(at) || self.bounce.is_some()
    }

    fn end(&self) -> Option<f64> {
        self.bounce.as_ref().unwrap_or(&self.glide).end()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Layer {
    pub(crate) parent: Option<LayerId>,
    pub(crate) frame: Option<Rect>,
    pub(crate) props: [Value; LayerProp::ALL.len()],
    pub(crate) flip: [Animated; 4],
    pub(crate) transition: Option<Timing>,
    pub(crate) layout_transition: Option<LayoutTransition>,
    pub(crate) keyframes: Option<(Keyframes, f64)>,
    pub(crate) effects: BTreeMap<EffectSlot, Applied>,
    pub(crate) rasterize: bool,
    pub(crate) surface: Option<SurfaceId>,
    pub(crate) scroll: Option<ScrollState>,
    pub(crate) source: Option<u64>,
    pub(crate) link: Option<(u64, ScrollLinked)>,
    pub(crate) fling: [Fling; 2],
    pub(crate) nested: Vec<Nested>,
    pub(crate) orphans: Vec<LayerId>,
    pub(crate) exiting: bool,
    pub(crate) animating: bool,
    pub(crate) wake: f64,
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            parent: None,
            frame: None,
            props: LayerProp::ALL.map(|prop| Value::Own(Animated::new(prop.initial()))),
            flip: [0.0, 0.0, 1.0, 1.0].map(Animated::new),
            transition: None,
            layout_transition: None,
            keyframes: None,
            effects: BTreeMap::new(),
            rasterize: false,
            surface: None,
            scroll: None,
            source: None,
            link: None,
            fling: [Fling::rest(0.0); 2],
            nested: Vec::new(),
            orphans: Vec::new(),
            exiting: false,
            animating: false,
            wake: f64::INFINITY,
        }
    }
}

impl Layer {
    fn value(&self, prop: LayerProp, motions: &Motions, at: f64) -> f32 {
        let keyed = self.keyframes.as_ref().and_then(|(keyframes, start)| {
            keyframes.sample(prop, keyframes.progress((at - start).max(0.0) as f32))
        });
        keyed.unwrap_or_else(|| match self.props[prop as usize] {
            Value::Own(animated) => animated.value(at),
            Value::Motion(id) => motions.get(&id).map_or(prop.initial(), |m| m.value(at)),
        })
    }

    fn own(&mut self, prop: LayerProp, motions: &Motions, at: f64) -> &mut Animated {
        if let Value::Motion(_) = self.props[prop as usize] {
            self.props[prop as usize] = Value::Own(Animated::new(self.value(prop, motions, at)));
        }
        match &mut self.props[prop as usize] {
            Value::Own(animated) => animated,
            Value::Motion(_) => unreachable!("the prop was just detached from its motion"),
        }
    }

    fn freeze(&mut self, motions: &Motions, at: f64) {
        if let Some((keyframes, start)) = self.keyframes.take() {
            self.bake(&keyframes, keyframes.progress((at - start).max(0.0) as f32));
        }
        for prop in LayerProp::ALL {
            self.own(prop, motions, at);
        }
    }

    fn bake(&mut self, keyframes: &Keyframes, progress: f32) {
        for prop in LayerProp::ALL {
            if let Some(value) = keyframes.sample(prop, progress) {
                self.props[prop as usize] = Value::Own(Animated::new(value));
            }
        }
    }

    pub(crate) fn pose(&self, motions: &Motions, at: f64) -> LayerPose {
        let value = |prop| self.value(prop, motions, at);
        let [x, y, width, height] = self.flip.map(|flip| flip.value(at));
        LayerPose {
            translate: Point::new(
                value(LayerProp::TranslateX) + x,
                value(LayerProp::TranslateY) + y,
            ),
            scroll: self.offset(at).unwrap_or(Point::ZERO),
            stretch: Size::new(width, height),
            scale: value(LayerProp::Scale),
            rotate: value(LayerProp::Rotate),
            opacity: value(LayerProp::Opacity).clamp(0.0, 1.0),
        }
    }

    pub(crate) fn fill(&self, motions: &Motions, at: f64) -> [f32; 4] {
        LayerProp::FILL.map(|prop| self.value(prop, motions, at))
    }

    pub(crate) fn radii(&self, motions: &Motions, at: f64) -> Corners {
        let [top_left, top_right, bottom_right, bottom_left] =
            LayerProp::RADII.map(|prop| self.value(prop, motions, at).max(0.0));
        Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    fn offset(&self, at: f64) -> Option<Point> {
        let scroll = self.scroll.as_ref()?;
        let stored = scroll.offset.to_array();
        let [x, y] = [0, 1].map(|axis| {
            let fling = &self.fling[axis];
            if fling.is_animating() {
                fling.state(at).0
            } else {
                stored[axis]
            }
        });
        Some(Point::new(x, y))
    }

    fn scroll_state(&self, at: f64) -> Option<ScrollState> {
        let offset = self.offset(at)?;
        self.scroll.map(|scroll| ScrollState { offset, ..scroll })
    }

    pub(crate) fn bounds(&self) -> Rect {
        Rect {
            origin: Point::ZERO,
            size: self.frame.unwrap_or_default().size,
        }
    }

    pub(crate) fn local(&self, pose: &LayerPose, scale: f32) -> Affine {
        let frame = self.frame.unwrap_or_default();
        let snap = |v: f32| (v * scale).round() / scale;
        let anchor = frame.origin - pose.scroll;
        pose.shape(self.bounds()).then(&Affine::translate(
            snap(anchor.x) + pose.translate.x,
            snap(anchor.y) + pose.translate.y,
        ))
    }

    pub(crate) fn stages(&self) -> impl Iterator<Item = &Applied> {
        self.effects.range(..EffectSlot::Backdrop).map(|(_, a)| a)
    }

    fn props_animating(&self) -> bool {
        self.props
            .iter()
            .any(|value| matches!(value, Value::Own(a) if a.is_animating()))
    }

    fn halt(&mut self, at: f64) -> Option<&mut ScrollState> {
        let offset = self.offset(at)?;
        self.fling = offset.to_array().map(Fling::rest);
        let scroll = self.scroll.as_mut()?;
        scroll.offset = offset;
        Some(scroll)
    }

    fn drag(&mut self, delta: Point, at: f64) {
        let Some(scroll) = self.halt(at) else {
            return;
        };
        let (max, viewport) = (scroll.max_offset(), scroll.viewport);
        let pull = |offset: f32, delta: f32, max: f32, extent: f32| {
            if max <= 0.0 {
                return offset;
            }
            let over = (-offset).max(offset - max).max(0.0);
            let outward = (offset <= 0.0 && delta < 0.0) || (offset >= max && delta > 0.0);
            let give = 1.0 + over / (RUBBER_BAND * extent.max(1.0));
            offset
                + if outward {
                    delta / (give * give)
                } else {
                    delta
                }
        };
        scroll.offset = Point::new(
            pull(scroll.offset.x, delta.x, max.x, viewport.width),
            pull(scroll.offset.y, delta.y, max.y, viewport.height),
        );
        self.fling = scroll.offset.to_array().map(Fling::rest);
    }

    fn scroll_to(&mut self, target: impl FnOnce(Point) -> Point, at: f64) {
        if let Some(scroll) = self.halt(at) {
            scroll.offset = scroll.clamp(target(scroll.offset));
            self.fling = scroll.offset.to_array().map(Fling::rest);
        }
    }

    fn release(&mut self, velocity: Point, at: f64) -> bool {
        let Some(scroll) = self.halt(at) else {
            return false;
        };
        let (offset, max) = (scroll.offset.to_array(), scroll.max_offset().to_array());
        let axes = offset.into_iter().zip(velocity.to_array()).zip(max);
        for (fling, ((offset, velocity), max)) in self.fling.iter_mut().zip(axes) {
            *fling = Fling::release(offset, velocity, max, at);
        }
        self.fling.iter().any(Fling::is_animating)
    }

    fn rebound(&mut self, at: f64) -> bool {
        let Some(scroll) = self.scroll else {
            return false;
        };
        let max = scroll.max_offset().to_array();
        for (fling, max) in self.fling.iter_mut().zip(max) {
            if fling.is_animating() && fling.is_gliding(at) {
                let (offset, velocity) = fling.state(at);
                *fling = Fling::release(offset, velocity, max, at);
            }
        }
        self.fling.iter().any(Fling::is_animating)
    }

    fn settle(&mut self, at: f64) -> bool {
        let mut moving = false;
        for value in &mut self.props {
            if let Value::Own(animated) = value {
                moving |= animated.settle(at);
            }
        }
        for animated in &mut self.flip {
            moving |= animated.settle(at);
        }
        for applied in self.effects.values_mut() {
            moving |= applied.settle(at);
        }
        if self.fling.iter().any(Fling::is_animating) {
            if let Some(offset) = self.offset(at)
                && let Some(scroll) = &mut self.scroll
            {
                scroll.offset = offset;
            }
            for fling in &mut self.fling {
                moving |= fling.settle(at);
            }
        }
        if let Some((keyframes, start)) = self.keyframes.take() {
            if keyframes.is_finished((at - start).max(0.0) as f32) {
                self.bake(&keyframes, 1.0);
            } else {
                self.keyframes = Some((keyframes, start));
                moving = true;
            }
        }
        moving
    }

    fn prop_ends(&self) -> impl Iterator<Item = Option<f64>> + '_ {
        self.props.iter().map(|value| match value {
            Value::Own(animated) => animated.end(),
            Value::Motion(_) => None,
        })
    }

    fn keyframes_end(&self) -> Option<f64> {
        let (keyframes, start) = self.keyframes.as_ref()?;
        Some(if keyframes.is_finished(f32::INFINITY) {
            start + f64::from(keyframes.length())
        } else {
            f64::INFINITY
        })
    }

    fn end(&self) -> Option<f64> {
        latest(
            self.prop_ends()
                .chain(self.flip.iter().map(Animated::end))
                .chain(self.effects.values().map(Applied::end))
                .chain(self.fling.iter().map(Fling::end))
                .chain([self.keyframes_end()]),
        )
    }

    pub(crate) fn exited(&self, at: f64) -> bool {
        self.exiting && self.prop_ends().flatten().all(|end| end <= at)
    }

    fn first_end(&self) -> f64 {
        let fling = self.fling.iter().map(|fling| {
            fling
                .bounce
                .as_ref()
                .map_or_else(|| fling.glide.end(), |bounce| Some(bounce.start()))
        });
        let first = earliest(
            self.prop_ends()
                .chain(self.flip.iter().map(Animated::end))
                .chain(self.effects.values().map(Applied::first_end))
                .chain(fling)
                .chain([self.keyframes_end().filter(|end| end.is_finite())]),
        );
        let forever = self
            .effects
            .values()
            .any(|applied| applied.effect.clock().is_some())
            || self.keyframes.is_some();
        if first.is_infinite() && !forever {
            f64::NEG_INFINITY
        } else {
            first
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LayerDelta {
    time: f64,
    root: Option<LayerId>,
    settles: Option<f64>,
    layers: Vec<(LayerId, Option<Layer>)>,
    motions: Vec<(MotionId, Option<Animated>)>,
}

impl LayerDelta {
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty() && self.motions.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct LayerChanges {
    pub removed: Vec<LayerId>,
    pub surfaces: Vec<(SurfaceId, bool)>,
}

#[derive(Default)]
pub struct LayerModel {
    now: f64,
    ids: SlotMap<LayerId, ()>,
    layers: SecondaryMap<LayerId, Layer>,
    motions: Motions,
    root: Option<LayerId>,
    animating: Vec<LayerId>,
    moving: Vec<MotionId>,
    sources: FxHashMap<u64, LayerId>,
    surface_layers: usize,
    settles: Option<f64>,
    touched: SecondaryMap<LayerId, ()>,
    touched_motions: Vec<MotionId>,
}

impl LayerModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, delta: LayerDelta) -> LayerChanges {
        let mut changes = LayerChanges::default();
        for (id, layer) in delta.layers {
            let new = layer.as_ref().map(|layer| (layer.surface, layer.source));
            let old = if let Some(layer) = layer {
                self.layers.insert(id, layer)
            } else {
                changes.removed.push(id);
                self.layers.remove(id)
            };
            let (surface, source) = new.unwrap_or_default();
            let (was, key) = old.map_or((None, None), |old| (old.surface, old.source));
            if was != surface {
                changes.surfaces.extend(was.map(|was| (was, false)));
                changes
                    .surfaces
                    .extend(surface.map(|surface| (surface, true)));
                self.surface_layers += usize::from(surface.is_some());
                self.surface_layers -= usize::from(was.is_some());
            }
            if let Some(key) = key.filter(|key| self.sources.get(key) == Some(&id)) {
                self.sources.remove(&key);
            }
            if let Some(key) = source {
                self.sources.insert(key, id);
            }
        }
        for (id, motion) in delta.motions {
            match motion {
                Some(motion) => self.motions.insert(id, motion),
                None => self.motions.remove(&id),
            };
        }
        self.now = self.now.max(delta.time);
        self.root = delta.root;
        self.settles = delta.settles;
        changes
    }

    pub(super) fn take_delta(&mut self) -> LayerDelta {
        if !self.touched.is_empty() || !self.touched_motions.is_empty() {
            let moving = self.moving.iter();
            let motions = moving.filter_map(|id| self.motions.get(id)?.end());
            let animating = self.animating.iter();
            let layers = animating.filter_map(|id| self.layers.get(*id)?.end());
            self.settles = motions.chain(layers).reduce(f64::max);
        }
        let touched = self.touched.drain();
        let layers = touched.map(|(id, ())| (id, self.layers.get(id).cloned()));
        let layers = layers.collect();
        let mut motions = take(&mut self.touched_motions);
        motions.sort_unstable_by_key(|id| id.0);
        motions.dedup();
        LayerDelta {
            time: self.now,
            root: self.root,
            settles: self.settles,
            layers,
            motions: (motions.into_iter())
                .map(|id| (id, self.motions.get(&id).copied()))
                .collect(),
        }
    }

    pub fn time(&self) -> f64 {
        self.now
    }

    pub fn is_animating(&self) -> bool {
        !self.animating.is_empty() || !self.moving.is_empty()
    }

    pub fn is_moving(&self, id: MotionId) -> bool {
        self.motions.get(&id).is_some_and(Animated::is_animating)
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn has_surfaces(&self) -> bool {
        self.surface_layers > 0
    }

    pub fn root(&self) -> Option<LayerId> {
        self.root
    }

    pub fn pose(&self, id: LayerId) -> Option<LayerPose> {
        self.layers
            .get(id)
            .map(|layer| self.linked_pose(layer, self.now))
    }

    pub fn scroll(&self, id: LayerId) -> Option<ScrollState> {
        self.layers.get(id)?.scroll_state(self.now)
    }

    pub fn motion(&self, id: MotionId) -> Option<f32> {
        self.motions.get(&id).map(|motion| motion.value(self.now))
    }

    pub fn is_layer_moving(&self, id: LayerId) -> bool {
        self.layers.get(id).is_some_and(|layer| {
            layer.animating
                || layer
                    .props
                    .iter()
                    .any(|value| matches!(value, Value::Motion(motion) if self.is_moving(*motion)))
        })
    }

    pub fn settles_at(&self) -> Option<f64> {
        self.settles
    }

    pub fn motion_end(&self, id: MotionId) -> Option<f64> {
        self.motions.get(&id)?.end()
    }

    pub(crate) fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.get(id)
    }

    pub(crate) fn motions(&self) -> &Motions {
        &self.motions
    }

    pub(crate) fn linked_pose(&self, layer: &Layer, at: f64) -> LayerPose {
        let mut pose = layer.pose(&self.motions, at);
        if let Some((key, link)) = layer.link
            && let Some(offset) = self
                .sources
                .get(&key)
                .and_then(|source| self.layers.get(*source))
                .and_then(|source| source.offset(at))
        {
            let (x, y) = link.offset(offset);
            pose.translate.x += x;
            pose.translate.y += y;
        }
        pose
    }

    fn update(&mut self, id: LayerId, f: impl FnOnce(&mut Layer, &Motions, f64) -> bool) {
        let Some(layer) = self.layers.get_mut(id) else {
            return;
        };
        if f(layer, &self.motions, self.now) && !layer.animating {
            layer.animating = true;
            self.animating.push(id);
        }
        if layer.animating {
            layer.wake = layer.first_end();
        }
        self.touched.insert(id, ());
    }

    fn set(&mut self, id: LayerId, f: impl FnOnce(&mut Layer, f64)) {
        self.update(id, |layer, _, at| {
            f(layer, at);
            false
        });
    }

    pub fn create_layer(&mut self) -> LayerId {
        let id = self.ids.insert(());
        self.layers.insert(id, Layer::default());
        self.touched.insert(id, ());
        id
    }

    pub fn set_root(&mut self, id: LayerId) {
        self.root = Some(id);
    }

    pub fn set_prop(&mut self, id: LayerId, prop: LayerProp, value: f32, animate: bool) {
        self.update(id, |layer, motions, at| {
            let transition = layer.transition.filter(|_| animate);
            let animated = layer.own(prop, motions, at);
            match transition {
                Some(timing) => animated.animate_to(value, timing, at),
                None => *animated = Animated::new(value),
            }
            animated.is_animating()
        });
    }

    pub fn bind_prop(&mut self, id: LayerId, prop: LayerProp, motion: MotionId) {
        self.set(id, |layer, _| {
            layer.props[prop as usize] = Value::Motion(motion);
        });
    }

    pub fn set_transition(&mut self, id: LayerId, timing: Option<Timing>) {
        self.set(id, |layer, _| layer.transition = timing);
    }

    pub fn set_layout_transition(&mut self, id: LayerId, transition: Option<LayoutTransition>) {
        self.set(id, |layer, _| layer.layout_transition = transition);
    }

    pub fn animate(&mut self, id: LayerId, keyframes: Keyframes) {
        self.update(id, |layer, _, at| {
            layer.keyframes = Some((keyframes, at));
            true
        });
    }

    pub fn set_effect(
        &mut self,
        id: LayerId,
        slot: EffectSlot,
        effect: Option<EffectInstance>,
        animate: bool,
    ) {
        self.update(id, |layer, _, at| {
            let transition = layer.transition.filter(|_| animate);
            let running = (layer.effects.get_mut(&slot).zip(effect.as_ref()))
                .filter(|(applied, effect)| applied.effect.same_shader(effect));
            let clock = running.as_ref().map_or(at, |(applied, _)| applied.clock);
            let mut wake = effect.is_some_and(|effect| effect.clock().is_some());
            if let Some(((applied, effect), timing)) = running.zip(transition) {
                wake |= applied.retarget(effect, timing, at);
            } else if let Some(effect) = &effect {
                layer.effects.insert(slot, Applied::new(effect, clock));
            } else {
                layer.effects.remove(&slot);
            }
            wake
        });
    }

    pub fn set_rasterize(&mut self, id: LayerId, rasterize: bool) {
        self.set(id, |layer, _| layer.rasterize = rasterize);
    }

    pub fn set_surface(&mut self, id: LayerId, surface: Option<SurfaceId>) {
        let Some(layer) = self.layers.get_mut(id) else {
            return;
        };
        let was = replace(&mut layer.surface, surface);
        self.surface_layers += usize::from(surface.is_some());
        self.surface_layers -= usize::from(was.is_some());
        self.touched.insert(id, ());
    }

    pub fn set_scroll_source(&mut self, id: LayerId, key: u64) {
        if self.layers.contains_key(id) {
            self.sources.insert(key, id);
            self.set(id, |layer, _| layer.source = Some(key));
        }
    }

    pub fn set_scroll_link(&mut self, id: LayerId, link: Option<(u64, ScrollLinked)>) {
        self.set(id, |layer, _| layer.link = link);
    }

    pub fn set_frame(&mut self, id: LayerId, frame: Rect) {
        self.update(id, |layer, _, at| {
            let mut wake = false;
            if let (Some(old), Some(transition)) = (layer.frame, layer.layout_transition) {
                let moved = old.origin - frame.origin;
                let [x, y, width, height] = &mut layer.flip;
                for (flip, delta) in [x, y].into_iter().zip([moved.x, moved.y]) {
                    flip.shift(delta, at);
                    flip.animate_to(0.0, transition.timing, at);
                    wake |= flip.is_animating();
                }
                let sizes = [
                    (old.size.width, frame.size.width),
                    (old.size.height, frame.size.height),
                ];
                for (flip, (old, new)) in [width, height].into_iter().zip(sizes) {
                    if transition.size && old > 0.0 && new > 0.0 {
                        flip.stretch(old / new, at);
                        flip.animate_to(1.0, transition.timing, at);
                        wake |= flip.is_animating();
                    }
                }
            }
            layer.frame = Some(frame);
            wake
        });
    }

    pub(super) fn set_nested(&mut self, id: LayerId, nested: Vec<Nested>) {
        let layers = &mut self.layers;
        let Some(layer) = layers.get_mut(id) else {
            return;
        };
        let kept = |old: &Nested| nested.iter().any(|new| new.layer == old.layer);
        let dropped: Vec<_> = layer
            .nested
            .iter()
            .filter(|old| !kept(old))
            .map(|old| old.layer)
            .collect();
        layer.nested = nested;
        let adopted: Vec<_> = layer.nested.iter().map(|nested| nested.layer).collect();
        for child in adopted {
            if let Some(child) = layers.get_mut(child) {
                child.parent = Some(id);
            }
        }
        let leaving = |child: &LayerId| {
            layers
                .get(*child)
                .is_some_and(|c| c.exiting && c.parent == Some(id))
        };
        let orphans: Vec<_> = dropped.into_iter().filter(leaving).collect();
        if let Some(layer) = layers.get_mut(id) {
            layer.orphans.extend(orphans);
        }
        self.touched.insert(id, ());
    }

    pub fn set_scroll_extent(&mut self, id: LayerId, viewport: Size, content: Size) {
        self.update(id, |layer, _, at| {
            let scroll = layer.scroll.get_or_insert_default();
            (scroll.viewport, scroll.content) = (viewport, content);
            if !layer.fling.iter().any(Fling::is_animating) {
                scroll.offset = scroll.clamp(scroll.offset);
                return false;
            }
            layer.rebound(at)
        });
    }

    pub fn scroll_by(&mut self, id: LayerId, delta: Point) {
        self.set(id, |layer, at| layer.scroll_to(|offset| offset + delta, at));
    }

    pub fn scroll_to(&mut self, id: LayerId, offset: Point) {
        self.set(id, |layer, at| layer.scroll_to(|_| offset, at));
    }

    pub fn scroll_drag(&mut self, id: LayerId, delta: Point) {
        self.set(id, |layer, at| layer.drag(delta, at));
    }

    pub fn scroll_release(&mut self, id: LayerId, velocity: Point) {
        self.update(id, |layer, _, at| layer.release(velocity, at));
    }

    pub fn enter(&mut self, id: LayerId, from: Pose, timing: Timing) {
        self.update(id, |layer, motions, at| {
            for (prop, value) in from.iter() {
                let animated = layer.own(prop, motions, at);
                let target = animated.target();
                *animated = Animated::new(value);
                animated.animate_to(target, timing, at);
            }
            true
        });
    }

    pub fn exit(&mut self, id: LayerId, to: Pose, timing: Timing) {
        self.update(id, |layer, motions, at| {
            layer.freeze(motions, at);
            for (prop, value) in to.iter() {
                layer.own(prop, motions, at).animate_to(value, timing, at);
            }
            layer.exiting = true;
            true
        });
    }

    pub fn create_motion(&mut self, id: MotionId, value: f32) {
        self.motions.insert(id, Animated::new(value));
        self.touched_motions.push(id);
    }

    pub(super) fn remove_motion(&mut self, id: MotionId) {
        self.motions.remove(&id);
        self.touched_motions.push(id);
    }

    pub(super) fn drive_motion(
        &mut self,
        id: MotionId,
        drive: impl FnOnce(&mut Animated, f64),
    ) -> bool {
        let Some(motion) = self.motions.get_mut(&id) else {
            return false;
        };
        drive(motion, self.now);
        self.touched_motions.push(id);
        let moving = motion.is_animating();
        if moving && !self.moving.contains(&id) {
            self.moving.push(id);
        }
        moving
    }

    fn unlink(&mut self, id: LayerId) -> Option<Layer> {
        let layer = self.layers.remove(id)?;
        self.ids.remove(id);
        self.touched.insert(id, ());
        if layer.animating {
            self.animating.retain(|animated| *animated != id);
        }
        let parent = layer.parent.filter(|parent| {
            let orphans = self.layers.get_mut(*parent).map(|above| &mut above.orphans);
            orphans.is_some_and(|orphans| {
                let before = orphans.len();
                orphans.retain(|orphan| *orphan != id);
                orphans.len() != before
            })
        });
        if let Some(parent) = parent {
            self.touched.insert(parent, ());
        }
        if self.root == Some(id) {
            self.root = None;
        }
        self.surface_layers -= usize::from(layer.surface.is_some());
        if let Some(key) = layer.source
            && self.sources.get(&key) == Some(&id)
        {
            self.sources.remove(&key);
        }
        Some(layer)
    }

    pub fn remove_layer(&mut self, id: LayerId) {
        let Some(layer) = self.unlink(id) else { return };
        for orphan in layer.orphans {
            self.remove_subtree(orphan);
        }
    }

    fn remove_subtree(&mut self, id: LayerId) {
        let Some(layer) = self.unlink(id) else { return };
        let children = layer
            .nested
            .iter()
            .map(|nested| nested.layer)
            .chain(layer.orphans);
        for child in children {
            if self.layers.get(child).is_some_and(|c| c.parent == Some(id)) {
                self.remove_subtree(child);
            }
        }
    }

    pub(super) fn advance(&mut self, time: f64) {
        if time <= self.now {
            return;
        }
        self.now = time;
        let LayerModel {
            now,
            layers,
            motions,
            animating,
            moving,
            ..
        } = self;
        let at = *now;
        moving.retain(|id| motions.get_mut(id).is_some_and(|motion| motion.settle(at)));
        let mut exited = Vec::new();
        animating.retain(|id| {
            let Some(layer) = layers.get_mut(*id) else {
                return false;
            };
            if at < layer.wake {
                return true;
            }
            layer.animating = layer.settle(at);
            layer.wake = layer.first_end();
            if layer.exiting && !layer.props_animating() {
                exited.push(*id);
                layer.animating = false;
            }
            layer.animating
        });
        for id in exited {
            self.remove_subtree(id);
        }
    }
}
