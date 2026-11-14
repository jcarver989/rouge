use crate::animation::{
    Animated, Decay, Keyframes, LayerProp, LayoutTransition, MAX_TICK, Pose, Spring, Timing,
};
use crate::motion::Completion;
use rouge_render::{
    Affine, Corners, DrawList, EffectDraw, EffectInstance, Group, Item, LayerDraw, Point, Rect,
    Size, SurfaceDraw, SurfaceId, Surfaces, device_rect,
};
use rustc_hash::FxHasher;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hasher;
use std::mem::take;

const RUBBER_BAND: f32 = 0.25;
const BOUNCE: Spring = Spring::new(220.0, 30.0);
const STAGE_BITS: u32 = 8;
const CLOCK_PERIOD: f32 = 3600.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

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

    fn shape(&self, bounds: Rect) -> Affine {
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

#[derive(Default)]
pub struct Content {
    pub lists: Vec<DrawList>,
    pub nested: Vec<Nested>,
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
    fn offset(self, scroll: Point) -> (f32, f32) {
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

#[derive(Clone, Copy)]
enum Value {
    Own(Animated),
    Motion(MotionId),
}

struct Motion {
    value: Animated,
    waiting: Vec<Completion>,
}

impl Motion {
    fn settle(&mut self, completed: bool) {
        for done in self.waiting.drain(..) {
            done.complete(completed);
        }
    }
}

type Motions = HashMap<MotionId, Motion>;

#[derive(Clone, Copy)]
struct Applied {
    effect: EffectInstance,
    values: [Animated; EffectInstance::VALUES],
    elapsed: f32,
}

impl Applied {
    fn new(effect: &EffectInstance, elapsed: f32) -> Self {
        Applied {
            effect: *effect,
            values: effect.values().map(Animated::new),
            elapsed,
        }
    }

    fn instance(&self) -> EffectInstance {
        self.effect
            .with_values(self.values.map(|value| value.value()))
            .at(self.elapsed)
    }

    fn retarget(&mut self, effect: &EffectInstance, timing: Timing) -> bool {
        let mut moving = false;
        for (value, target) in self.values.iter_mut().zip(effect.values()) {
            value.animate_to(*target, timing);
            moving |= value.is_animating();
        }
        moving
    }

    fn tick(&mut self, dt: f32) -> bool {
        let moving = self
            .values
            .iter_mut()
            .fold(false, |moving, value| value.tick(dt) | moving);
        if self.effect.clock().is_some() {
            self.elapsed = (self.elapsed + dt.max(0.0)) % CLOCK_PERIOD;
        }
        moving || self.effect.clock().is_some()
    }
}

struct Layer {
    parent: Option<LayerId>,
    frame: Option<Rect>,
    props: [Value; LayerProp::ALL.len()],
    flip: [Animated; 4],
    transition: Option<Timing>,
    layout_transition: Option<LayoutTransition>,
    keyframes: Option<(Keyframes, f32)>,
    effects: BTreeMap<EffectSlot, Applied>,
    rasterize: bool,
    surface: Option<SurfaceId>,
    scroll: Option<ScrollState>,
    source: Option<u64>,
    link: Option<(u64, ScrollLinked)>,
    fling: [Animated; 2],
    lists: Vec<DrawList>,
    nested: Vec<Nested>,
    orphans: Vec<LayerId>,
    exiting: bool,
    animating: bool,
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
            fling: [Animated::new(0.0); 2],
            lists: Vec::new(),
            nested: Vec::new(),
            orphans: Vec::new(),
            exiting: false,
            animating: false,
        }
    }
}

impl Layer {
    fn value(&self, prop: LayerProp, motions: &Motions) -> f32 {
        let keyed = self
            .keyframes
            .as_ref()
            .and_then(|(keyframes, elapsed)| keyframes.sample(prop, keyframes.progress(*elapsed)));
        keyed.unwrap_or_else(|| match self.props[prop as usize] {
            Value::Own(animated) => animated.value(),
            Value::Motion(id) => motions.get(&id).map_or(prop.initial(), |m| m.value.value()),
        })
    }

    fn own(&mut self, prop: LayerProp, motions: &Motions) -> &mut Animated {
        if let Value::Motion(_) = self.props[prop as usize] {
            self.props[prop as usize] = Value::Own(Animated::new(self.value(prop, motions)));
        }
        match &mut self.props[prop as usize] {
            Value::Own(animated) => animated,
            Value::Motion(_) => unreachable!("the prop was just detached from its motion"),
        }
    }

    fn freeze(&mut self, motions: &Motions) {
        if let Some((keyframes, elapsed)) = self.keyframes.take() {
            self.bake(&keyframes, keyframes.progress(elapsed));
        }
        for prop in LayerProp::ALL {
            self.own(prop, motions);
        }
    }

    fn bake(&mut self, keyframes: &Keyframes, progress: f32) {
        for prop in LayerProp::ALL {
            if let Some(value) = keyframes.sample(prop, progress) {
                self.props[prop as usize] = Value::Own(Animated::new(value));
            }
        }
    }

    fn pose(&self, motions: &Motions) -> LayerPose {
        let value = |prop| self.value(prop, motions);
        LayerPose {
            translate: Point::new(
                value(LayerProp::TranslateX) + self.flip[0].value(),
                value(LayerProp::TranslateY) + self.flip[1].value(),
            ),
            scroll: self.scroll.map_or(Point::ZERO, |scroll| scroll.offset),
            stretch: Size::new(self.flip[2].value(), self.flip[3].value()),
            scale: value(LayerProp::Scale),
            rotate: value(LayerProp::Rotate),
            opacity: value(LayerProp::Opacity).clamp(0.0, 1.0),
        }
    }

    fn fill(&self, motions: &Motions) -> [f32; 4] {
        LayerProp::FILL.map(|prop| self.value(prop, motions))
    }

    fn radii(&self, motions: &Motions) -> Corners {
        let [top_left, top_right, bottom_right, bottom_left] =
            LayerProp::RADII.map(|prop| self.value(prop, motions).max(0.0));
        Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    fn bounds(&self) -> Rect {
        Rect {
            origin: Point::ZERO,
            size: self.frame.unwrap_or_default().size,
        }
    }

    fn local(&self, pose: &LayerPose, scale: f32) -> Affine {
        let frame = self.frame.unwrap_or_default();
        let snap = |v: f32| (v * scale).round() / scale;
        let anchor = frame.origin - pose.scroll;
        pose.shape(self.bounds()).then(&Affine::translate(
            snap(anchor.x) + pose.translate.x,
            snap(anchor.y) + pose.translate.y,
        ))
    }

    fn stages(&self) -> impl Iterator<Item = &Applied> {
        self.effects.range(..EffectSlot::Backdrop).map(|(_, a)| a)
    }

    fn props_animating(&self) -> bool {
        self.props
            .iter()
            .any(|value| matches!(value, Value::Own(a) if a.is_animating()))
    }

    fn drag(&mut self, delta: Point) {
        let Some(scroll) = &mut self.scroll else {
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
        self.fling = scroll.offset.to_array().map(Animated::new);
    }

    fn scroll_to(&mut self, target: impl FnOnce(Point) -> Point) {
        if let Some(scroll) = &mut self.scroll {
            scroll.offset = scroll.clamp(target(scroll.offset));
            self.fling = scroll.offset.to_array().map(Animated::new);
        }
    }

    fn release(&mut self, velocity: Point) -> bool {
        let Some(scroll) = &self.scroll else {
            return false;
        };
        let (offset, max) = (scroll.offset.to_array(), scroll.max_offset().to_array());
        let axes = offset.into_iter().zip(velocity.to_array()).zip(max);
        for (fling, ((offset, velocity), max)) in self.fling.iter_mut().zip(axes) {
            *fling = Animated::new(offset);
            if offset < 0.0 || offset > max {
                fling.fling(velocity, offset.clamp(0.0, max), BOUNCE);
            } else if max > 0.0 {
                fling.decay(velocity, Decay::new());
            }
        }
        self.fling.iter().any(Animated::is_animating)
    }

    fn tick(&mut self, dt: f32) -> bool {
        let mut moving = false;
        for value in &mut self.props {
            if let Value::Own(animated) = value {
                moving |= animated.tick(dt);
            }
        }
        for animated in &mut self.flip {
            moving |= animated.tick(dt);
        }
        for applied in self.effects.values_mut() {
            moving |= applied.tick(dt);
        }
        if let Some(scroll) = &mut self.scroll {
            let max = scroll.max_offset().to_array();
            let mut offset = scroll.offset.to_array();
            for ((fling, offset), max) in self.fling.iter_mut().zip(&mut offset).zip(max) {
                if !fling.is_animating() {
                    continue;
                }
                fling.tick(dt);
                let value = fling.value();
                if fling.is_decaying() && (value < 0.0 || value > max) {
                    fling.animate_to(value.clamp(0.0, max), BOUNCE.into());
                }
                *offset = value;
                moving |= fling.is_animating();
            }
            scroll.offset = Point::new(offset[0], offset[1]);
        }
        if let Some((keyframes, elapsed)) = self.keyframes.take() {
            let elapsed = elapsed + dt.clamp(0.0, MAX_TICK);
            if keyframes.is_finished(elapsed) {
                self.bake(&keyframes, 1.0);
            } else {
                self.keyframes = Some((keyframes, elapsed));
            }
            moving = true;
        }
        moving
    }
}

#[derive(Default)]
pub struct Compositor {
    layers: HashMap<LayerId, Layer>,
    motions: Motions,
    root: Option<LayerId>,
    animating: Vec<LayerId>,
    moving: Vec<MotionId>,
    sources: HashMap<u64, LayerId>,
    surface_uses: Vec<(SurfaceId, bool)>,
    surface_layers: usize,
    empty: DrawList,
}

impl Compositor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_animating(&self) -> bool {
        !self.animating.is_empty() || !self.moving.is_empty()
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn has_surfaces(&self) -> bool {
        self.surface_layers > 0
    }

    pub fn sync_surfaces(&mut self, surfaces: &mut Surfaces) {
        for (id, bound) in take(&mut self.surface_uses) {
            if bound {
                surfaces.bind(id);
            } else {
                surfaces.unbind(id);
            }
        }
    }

    pub fn pose(&self, id: LayerId) -> Option<LayerPose> {
        self.layers.get(&id).map(|layer| self.linked_pose(layer))
    }

    fn linked_pose(&self, layer: &Layer) -> LayerPose {
        let mut pose = layer.pose(&self.motions);
        if let Some((key, link)) = layer.link
            && let Some(scroll) = self
                .sources
                .get(&key)
                .and_then(|source| self.layers.get(source))
                .and_then(|source| source.scroll)
        {
            let (x, y) = link.offset(scroll.offset);
            pose.translate.x += x;
            pose.translate.y += y;
        }
        pose
    }

    pub fn scroll(&self, id: LayerId) -> Option<ScrollState> {
        self.layers.get(&id)?.scroll
    }

    pub fn motion(&self, id: MotionId) -> Option<f32> {
        self.motions.get(&id).map(|motion| motion.value.value())
    }

    pub fn create_layer(&mut self, id: LayerId) {
        self.layers.insert(id, Layer::default());
    }

    pub fn set_root(&mut self, id: LayerId) {
        self.root = Some(id);
    }

    fn update(&mut self, id: LayerId, f: impl FnOnce(&mut Layer, &Motions) -> bool) {
        let Some(layer) = self.layers.get_mut(&id) else {
            return;
        };
        if f(layer, &self.motions) && !layer.animating {
            layer.animating = true;
            self.animating.push(id);
        }
    }

    pub fn set_prop(&mut self, id: LayerId, prop: LayerProp, value: f32, animate: bool) {
        self.update(id, |layer, motions| {
            let transition = layer.transition.filter(|_| animate);
            let animated = layer.own(prop, motions);
            match transition {
                Some(timing) => animated.animate_to(value, timing),
                None => *animated = Animated::new(value),
            }
            animated.is_animating()
        });
    }

    pub fn bind_prop(&mut self, id: LayerId, prop: LayerProp, motion: MotionId) {
        self.update(id, |layer, _| {
            layer.props[prop as usize] = Value::Motion(motion);
            false
        });
    }

    pub fn set_transition(&mut self, id: LayerId, timing: Option<Timing>) {
        self.update(id, |layer, _| {
            layer.transition = timing;
            false
        });
    }

    pub fn set_layout_transition(&mut self, id: LayerId, transition: Option<LayoutTransition>) {
        self.update(id, |layer, _| {
            layer.layout_transition = transition;
            false
        });
    }

    pub fn animate(&mut self, id: LayerId, keyframes: Keyframes) {
        self.update(id, |layer, _| {
            layer.keyframes = Some((keyframes, 0.0));
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
        self.update(id, |layer, _| {
            let transition = layer.transition.filter(|_| animate);
            let running = (layer.effects.get_mut(&slot).zip(effect.as_ref()))
                .filter(|(applied, effect)| applied.effect.same_shader(effect));
            let elapsed = running.as_ref().map_or(0.0, |(applied, _)| applied.elapsed);
            let mut wake = effect.is_some_and(|effect| effect.clock().is_some());
            if let Some(((applied, effect), timing)) = running.zip(transition) {
                wake |= applied.retarget(effect, timing);
            } else if let Some(effect) = &effect {
                layer.effects.insert(slot, Applied::new(effect, elapsed));
            } else {
                layer.effects.remove(&slot);
            }
            wake
        });
    }

    pub fn set_rasterize(&mut self, id: LayerId, rasterize: bool) {
        self.update(id, |layer, _| {
            layer.rasterize = rasterize;
            false
        });
    }

    pub fn set_surface(&mut self, id: LayerId, surface: Option<SurfaceId>) {
        let Some(layer) = self.layers.get_mut(&id) else {
            return;
        };
        if layer.surface == surface {
            return;
        }
        if let Some(old) = layer.surface.take() {
            self.surface_uses.push((old, false));
            self.surface_layers -= 1;
        }
        if let Some(new) = surface {
            self.surface_uses.push((new, true));
            self.surface_layers += 1;
        }
        layer.surface = surface;
    }

    pub fn set_scroll_source(&mut self, id: LayerId, key: u64) {
        if let Some(layer) = self.layers.get_mut(&id) {
            layer.source = Some(key);
            self.sources.insert(key, id);
        }
    }

    pub fn set_scroll_link(&mut self, id: LayerId, link: Option<(u64, ScrollLinked)>) {
        self.update(id, |layer, _| {
            layer.link = link;
            false
        });
    }

    pub fn set_frame(&mut self, id: LayerId, frame: Rect) {
        self.update(id, |layer, _| {
            let mut wake = false;
            if let (Some(old), Some(transition)) = (layer.frame, layer.layout_transition) {
                let moved = old.origin - frame.origin;
                let [x, y, width, height] = &mut layer.flip;
                for (flip, delta) in [x, y].into_iter().zip([moved.x, moved.y]) {
                    flip.shift(delta);
                    flip.animate_to(0.0, transition.timing);
                    wake |= flip.is_animating();
                }
                let sizes = [
                    (old.size.width, frame.size.width),
                    (old.size.height, frame.size.height),
                ];
                for (flip, (old, new)) in [width, height].into_iter().zip(sizes) {
                    if transition.size && old > 0.0 && new > 0.0 {
                        flip.stretch(old / new);
                        flip.animate_to(1.0, transition.timing);
                        wake |= flip.is_animating();
                    }
                }
            }
            layer.frame = Some(frame);
            wake
        });
    }

    pub fn set_content(&mut self, id: LayerId, content: Content) {
        let layers = &mut self.layers;
        let Some(layer) = layers.get_mut(&id) else {
            return;
        };
        layer.lists.truncate(content.lists.len());
        for (index, list) in content.lists.into_iter().enumerate() {
            match layer.lists.get_mut(index) {
                Some(existing) => existing.replace(list),
                None => layer.lists.push(list),
            }
        }
        let kept = |old: &Nested| content.nested.iter().any(|new| new.layer == old.layer);
        let dropped: Vec<_> = layer
            .nested
            .iter()
            .filter(|old| !kept(old))
            .map(|old| old.layer)
            .collect();
        layer.nested = content.nested;
        let adopted: Vec<_> = layer.nested.iter().map(|nested| nested.layer).collect();
        for child in adopted {
            if let Some(child) = layers.get_mut(&child) {
                child.parent = Some(id);
            }
        }
        let leaving = |child: &LayerId| {
            layers
                .get(child)
                .is_some_and(|c| c.exiting && c.parent == Some(id))
        };
        let orphans: Vec<_> = dropped.into_iter().filter(leaving).collect();
        if let Some(layer) = layers.get_mut(&id) {
            layer.orphans.extend(orphans);
        }
    }

    pub fn set_scroll_extent(&mut self, id: LayerId, viewport: Size, content: Size) {
        self.update(id, |layer, _| {
            let scroll = layer.scroll.get_or_insert_default();
            (scroll.viewport, scroll.content) = (viewport, content);
            if !layer.fling.iter().any(Animated::is_animating) {
                scroll.offset = scroll.clamp(scroll.offset);
            }
            false
        });
    }

    pub fn scroll_by(&mut self, id: LayerId, delta: Point) {
        self.update(id, |layer, _| {
            layer.scroll_to(|offset| offset + delta);
            false
        });
    }

    pub fn scroll_to(&mut self, id: LayerId, offset: Point) {
        self.update(id, |layer, _| {
            layer.scroll_to(|_| offset);
            false
        });
    }

    pub fn scroll_drag(&mut self, id: LayerId, delta: Point) {
        self.update(id, |layer, _| {
            layer.drag(delta);
            false
        });
    }

    pub fn scroll_release(&mut self, id: LayerId, velocity: Point) {
        self.update(id, |layer, _| layer.release(velocity));
    }

    pub fn enter(&mut self, id: LayerId, from: Pose, timing: Timing) {
        self.update(id, |layer, motions| {
            for (prop, value) in from.iter() {
                let animated = layer.own(prop, motions);
                let target = animated.target();
                *animated = Animated::new(value);
                animated.animate_to(target, timing);
            }
            true
        });
    }

    pub fn exit(&mut self, id: LayerId, to: Pose, timing: Timing) {
        self.update(id, |layer, motions| {
            layer.freeze(motions);
            for (prop, value) in to.iter() {
                layer.own(prop, motions).animate_to(value, timing);
            }
            layer.exiting = true;
            true
        });
    }

    pub fn create_motion(&mut self, id: MotionId, value: f32) {
        let motion = Motion {
            value: Animated::new(value),
            waiting: Vec::new(),
        };
        self.motions.insert(id, motion);
    }

    fn interrupt(&mut self, id: MotionId) -> Option<&mut Motion> {
        let motion = self.motions.get_mut(&id)?;
        motion.settle(false);
        Some(motion)
    }

    pub fn set_motion(&mut self, id: MotionId, value: f32) {
        if let Some(motion) = self.interrupt(id) {
            motion.value = Animated::new(value);
        }
    }

    pub fn stop_motion(&mut self, id: MotionId) {
        if let Some(motion) = self.interrupt(id) {
            motion.value = Animated::new(motion.value.value());
        }
    }

    pub fn remove_motion(&mut self, id: MotionId) {
        self.interrupt(id);
        self.motions.remove(&id);
    }

    pub fn animate_motion(&mut self, id: MotionId, target: f32, timing: Timing, done: Completion) {
        self.drive(id, done, |value| value.animate_to(target, timing));
    }

    pub fn decay_motion(&mut self, id: MotionId, velocity: f32, decay: Decay, done: Completion) {
        self.drive(id, done, |value| value.decay(velocity, decay));
    }

    fn drive(&mut self, id: MotionId, done: Completion, start: impl FnOnce(&mut Animated)) {
        let Some(motion) = self.interrupt(id) else {
            return;
        };
        start(&mut motion.value);
        motion.waiting.push(done);
        if !motion.value.is_animating() {
            motion.settle(true);
        } else if !self.moving.contains(&id) {
            self.moving.push(id);
        }
    }

    fn unlink(&mut self, id: LayerId) -> Option<Layer> {
        let layer = self.layers.remove(&id)?;
        if layer.animating {
            self.animating.retain(|animated| *animated != id);
        }
        if let Some(parent) = layer.parent.and_then(|parent| self.layers.get_mut(&parent)) {
            parent.orphans.retain(|orphan| *orphan != id);
        }
        if self.root == Some(id) {
            self.root = None;
        }
        if let Some(surface) = layer.surface {
            self.surface_uses.push((surface, false));
            self.surface_layers -= 1;
        }
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
            if self
                .layers
                .get(&child)
                .is_some_and(|c| c.parent == Some(id))
            {
                self.remove_subtree(child);
            }
        }
    }

    pub fn tick(&mut self, dt: f32) {
        let Compositor {
            layers,
            motions,
            animating,
            moving,
            ..
        } = self;
        moving.retain(|id| {
            let Some(motion) = motions.get_mut(id) else {
                return false;
            };
            if motion.value.is_animating() && !motion.value.tick(dt) {
                motion.settle(true);
            }
            motion.value.is_animating()
        });
        let mut exited = Vec::new();
        animating.retain(|id| {
            let Some(layer) = layers.get_mut(id) else {
                return false;
            };
            layer.animating = layer.tick(dt);
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

    pub fn scene(&self, scale: f32) -> Vec<Item<'_>> {
        let mut items = Vec::new();
        if let Some(root) = self.root {
            let parent = Parent {
                transform: Affine::IDENTITY,
                opacity: 1.0,
                scissor: None,
            };
            self.emit(root, None, scale, &parent, &mut items);
        }
        items
    }

    fn emit<'a>(
        &'a self,
        id: LayerId,
        clip: Option<Rect>,
        scale: f32,
        parent: &Parent,
        out: &mut Vec<Item<'a>>,
    ) {
        let Some(layer) = self.layers.get(&id) else {
            return;
        };
        let pose = self.linked_pose(layer);
        if pose.opacity <= 0.0 {
            return;
        }
        let scissor = match clip {
            Some(clip) => {
                let clip = device_rect(&parent.transform, &clip, scale);
                Some(
                    parent
                        .scissor
                        .map_or(clip, |scissor| scissor.intersect(&clip)),
                )
            }
            None => parent.scissor,
        };
        if scissor.is_some_and(|scissor| scissor.is_empty()) {
            return;
        }
        let transform = layer.local(&pose, scale).then(&parent.transform);
        let radii = layer.radii(&self.motions);
        let shaped = |applied: &Applied| EffectDraw {
            effect: applied.instance(),
            bounds: layer.bounds(),
            corner_radii: radii,
        };
        let style = Style {
            fill: layer.fill(&self.motions),
            radii,
            backdrop: layer.effects.get(&EffectSlot::Backdrop).map(shaped),
        };
        let stages: Vec<_> = layer.stages().map(shaped).collect();
        let grouped = !stages.is_empty()
            || ((layer.rasterize || (pose.opacity < 1.0 && self.overlaps(layer)))
                && !self.has_nested_backdrop(layer));
        let opacity = pose.opacity * parent.opacity;
        if !grouped {
            let this = Parent {
                transform,
                opacity,
                scissor,
            };
            self.emit_content(layer, style, scale, &this, out);
            return;
        }
        let mut items = Vec::new();
        let this = Parent {
            transform,
            opacity: 1.0,
            scissor,
        };
        self.emit_content(
            layer,
            Style {
                backdrop: None,
                ..style
            },
            scale,
            &this,
            &mut items,
        );
        if style.backdrop.is_some() {
            let draw = LayerDraw {
                backdrop: style.backdrop,
                ..LayerDraw::new(&self.empty)
            };
            out.push(Item::Layer(LayerDraw {
                transform,
                opacity,
                scissor,
                ..draw
            }));
        }
        let last = stages.len().saturating_sub(1);
        let effects = stages
            .into_iter()
            .map(Some)
            .chain([None])
            .take(last + 1)
            .enumerate();
        for (stage, effect) in effects {
            let mut hasher = FxHasher::default();
            let Some(bounds) = summarize(&items, scale, &mut hasher) else {
                return;
            };
            let group = Group {
                key: (id.0 << STAGE_BITS) | stage as u64,
                signature: hasher.finish(),
                bounds,
                opacity: if stage == last { opacity } else { 1.0 },
                scissor,
                transform,
                effect,
                items,
            };
            items = vec![Item::Group(group)];
        }
        out.extend(items);
    }

    fn emit_content<'a>(
        &'a self,
        layer: &'a Layer,
        mut style: Style,
        scale: f32,
        this: &Parent,
        out: &mut Vec<Item<'a>>,
    ) {
        let mut surface = layer.surface.map(|id| {
            Item::Surface(SurfaceDraw {
                id,
                bounds: layer.bounds(),
                transform: this.transform,
                opacity: this.opacity,
                radii: style.radii,
                scissor: this.scissor,
            })
        });
        for (segment, list) in layer.lists.iter().enumerate() {
            if !list.is_empty() || style.backdrop.is_some() {
                out.push(Item::Layer(LayerDraw {
                    list,
                    transform: this.transform,
                    opacity: this.opacity,
                    fill: style.fill,
                    radii: style.radii,
                    backdrop: style.backdrop.take(),
                    scissor: this.scissor,
                }));
            }
            out.extend(surface.take());
            if let Some(nested) = layer.nested.get(segment) {
                self.emit(nested.layer, nested.clip, scale, this, out);
            }
        }
        out.extend(surface);
        for orphan in &layer.orphans {
            self.emit(*orphan, None, scale, this, out);
        }
    }

    fn children<'a>(&'a self, layer: &'a Layer) -> impl Iterator<Item = &'a Layer> {
        let ids = layer
            .nested
            .iter()
            .map(|nested| &nested.layer)
            .chain(&layer.orphans);
        ids.filter_map(|id| self.layers.get(id))
    }

    fn primitives(&self, layer: &Layer, limit: usize) -> usize {
        let mut count: usize = layer.lists.iter().map(DrawList::len).sum::<usize>()
            + usize::from(layer.surface.is_some());
        for child in self.children(layer) {
            if count >= limit {
                break;
            }
            count += self.primitives(child, limit - count);
        }
        count
    }

    fn overlaps(&self, layer: &Layer) -> bool {
        self.primitives(layer, 2) >= 2
    }

    fn has_nested_backdrop(&self, layer: &Layer) -> bool {
        let reads = |child: &Layer| child.effects.range(EffectSlot::Blend..).next().is_some();
        self.children(layer)
            .any(|child| reads(child) || self.has_nested_backdrop(child))
    }
}

struct Parent {
    transform: Affine,
    opacity: f32,
    scissor: Option<Rect>,
}

#[derive(Clone, Copy)]
struct Style {
    fill: [f32; 4],
    radii: Corners,
    backdrop: Option<EffectDraw>,
}

fn summarize(items: &[Item<'_>], scale: f32, hasher: &mut FxHasher) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    for item in items {
        item.fingerprint(hasher);
        if let Some(extent) = item.extent(scale) {
            bounds = Some(bounds.map_or(extent, |bounds| bounds.union(&extent)));
        }
    }
    bounds
}
