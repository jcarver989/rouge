use super::{ElementId, Tree};
use crate::animation::{Keyframes, LayerProp, LayoutTransition, Pose, Timing};
use crate::compositor::{EffectSlot, LayerId, MotionId, ScrollLinked};
use crate::motion::{MotionOp, MotionProp};
use crate::style::Dirty;
use rouge_render::{Color, Corners, EffectInstance, Rect, SurfaceId};

#[derive(Default)]
pub(crate) struct LayerSpec {
    pub props: Vec<(LayerProp, MotionProp)>,
    pub effects: Vec<(EffectSlot, EffectInstance)>,
    pub transition: Option<Timing>,
    pub layout_transition: Option<LayoutTransition>,
    pub keyframes: Option<Keyframes>,
    pub rasterize: bool,
    pub surface: Option<SurfaceId>,
    pub link: Option<(u64, ScrollLinked)>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Presence {
    pub enter: Option<(Pose, Timing)>,
    pub exit: Option<(Pose, Timing)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Slot {
    Own,
    Content,
}

pub(super) type LayerRef = (ElementId, Slot);

pub(super) struct LayerState {
    pub(super) id: LayerId,
    pub(super) dirty: bool,
    pub(super) frame: Option<Rect>,
    pub(super) nested: Vec<LayerRef>,
    backing: Option<(Color, Corners)>,
}

impl Tree {
    pub(super) fn new_layer(&mut self, spec: LayerSpec) -> LayerState {
        let id = LayerId(self.next_layer);
        self.next_layer += 1;
        let compositor = &mut self.compositor;
        compositor.create_layer(id);
        compositor.set_transition(id, spec.transition);
        compositor.set_layout_transition(id, spec.layout_transition);
        for (prop, value) in spec.props {
            match value {
                MotionProp::Motion(motion) => compositor.bind_prop(id, prop, motion.id()),
                MotionProp::Value(value) => compositor.set_prop(id, prop, value, false),
            }
        }
        for (slot, effect) in spec.effects {
            compositor.set_effect(id, slot, Some(effect), false);
        }
        if let Some(keyframes) = spec.keyframes {
            compositor.animate(id, keyframes);
        }
        compositor.set_rasterize(id, spec.rasterize);
        compositor.set_surface(id, spec.surface);
        compositor.set_scroll_link(id, spec.link);
        LayerState {
            id,
            dirty: true,
            frame: None,
            nested: Vec::new(),
            backing: None,
        }
    }

    pub(super) fn sync_backing(&mut self, id: ElementId) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let style = element.effective_style();
        let backing = (
            style.background.unwrap_or(Color::TRANSPARENT),
            style.corner_radii,
        );
        let Some(layer) = &mut element.layers[Slot::Own as usize] else {
            return;
        };
        let previous = layer.backing.replace(backing);
        if previous == Some(backing) {
            return;
        }
        let animate = previous.is_some() && layer.frame.is_some();
        for (prop, value) in Pose::new().background(backing.0).rounded(backing.1).iter() {
            self.compositor.set_prop(layer.id, prop, value, animate);
        }
        self.mark(Dirty::Composite);
    }

    pub(super) fn layer_id(&self, id: ElementId, slot: Slot) -> Option<LayerId> {
        Some(self.elements.get(id)?.layer(slot)?.id)
    }

    pub(crate) fn set_layer_prop(
        &mut self,
        id: ElementId,
        prop: LayerProp,
        value: MotionProp,
        animate: bool,
    ) {
        let Some(layer) = self.layer_id(id, Slot::Own) else {
            return;
        };
        match value {
            MotionProp::Motion(motion) => self.compositor.bind_prop(layer, prop, motion.id()),
            MotionProp::Value(value) => self.compositor.set_prop(layer, prop, value, animate),
        }
        self.mark(Dirty::Composite);
    }

    pub(crate) fn set_layer_effect(
        &mut self,
        id: ElementId,
        slot: EffectSlot,
        effect: EffectInstance,
    ) {
        if let Some(layer) = self.layer_id(id, Slot::Own) {
            self.compositor.set_effect(layer, slot, Some(effect), true);
            self.mark(Dirty::Composite);
        }
    }

    pub(crate) fn set_scroll_link(&mut self, id: ElementId, key: u64, link: ScrollLinked) {
        if let Some(layer) = self.layer_id(id, Slot::Own) {
            self.compositor.set_scroll_link(layer, Some((key, link)));
            self.mark(Dirty::Composite);
        }
    }

    pub(super) fn promote(&mut self, id: ElementId, presence: Presence) {
        let Some(element) = self.elements.get(id) else {
            return;
        };
        if element.layer(Slot::Own).is_none() {
            self.mark_layer(id, false);
            let layer = self.new_layer(LayerSpec::default());
            self.elements[id].layers[Slot::Own as usize] = Some(Box::new(layer));
            self.sync_backing(id);
            self.mark(Dirty::Layout);
        }
        if let (Some((from, timing)), Some(layer)) = (presence.enter, self.layer_id(id, Slot::Own))
        {
            self.compositor.enter(layer, from, timing);
            self.mark(Dirty::Composite);
        }
    }

    pub(crate) fn motion(&mut self, id: MotionId, op: MotionOp) {
        let compositor = &mut self.compositor;
        match op {
            MotionOp::Create(value) => {
                if value.is_alive() {
                    compositor.create_motion(id, value.get_untracked());
                    self.motions.insert(id, value);
                }
                return;
            }
            MotionOp::Set(value) => compositor.set_motion(id, value),
            MotionOp::Animate {
                target,
                timing,
                done,
            } => compositor.animate_motion(id, target, timing, done),
            MotionOp::Decay {
                velocity,
                decay,
                done,
            } => compositor.decay_motion(id, velocity, decay, done),
            MotionOp::Stop => compositor.stop_motion(id),
            MotionOp::Remove => {
                compositor.remove_motion(id);
                self.motions.remove(&id);
            }
        }
        self.mark(Dirty::Composite);
    }
}
