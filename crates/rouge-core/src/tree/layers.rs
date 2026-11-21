use super::{ElementId, Tree};
use crate::animation::{Keyframes, LayerProp, LayoutTransition, Pose, Timing};
use crate::compositor::{EffectSlot, LayerId, MotionId, ScrollLinked};
use crate::motion::{MotionOp, MotionProp};
use crate::style::Dirty;
use rouge_render::{Color, Corners, EffectInstance, Rect, SurfaceId};

#[derive(Default)]
pub(crate) struct LayerSpec {
    pub transition: Option<Timing>,
    pub keyframes: Option<Keyframes>,
    pub rasterize: bool,
    pub surface: Option<SurfaceId>,
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
        let layers = &mut self.layers;
        let id = layers.create_layer();
        layers.set_transition(id, spec.transition);
        if let Some(keyframes) = spec.keyframes {
            layers.animate(id, keyframes);
        }
        layers.set_rasterize(id, spec.rasterize);
        layers.set_surface(id, spec.surface);
        LayerState {
            id,
            dirty: true,
            frame: None,
            nested: Vec::new(),
            backing: None,
        }
    }

    pub(super) fn sync_backing_subtree(&mut self, id: ElementId) {
        self.sync_backing(id);
        let children = self.elements[id].children.clone();
        for child in children {
            self.sync_backing_subtree(child);
        }
    }

    pub(super) fn sync_backing(&mut self, id: ElementId) {
        if self
            .elements
            .get(id)
            .is_none_or(|element| element.layer(Slot::Own).is_none())
        {
            return;
        }
        let palette = self.palette_at(id);
        let element = &mut self.elements[id];
        let style = element.effective_style();
        let backing = (
            style
                .background
                .map_or(Color::TRANSPARENT, |fill| fill.resolve(&palette)),
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
            self.layers.set_prop(layer.id, prop, value, animate);
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
            MotionProp::Motion(motion) => self.layers.bind_prop(layer, prop, motion.id()),
            MotionProp::Mapped(mapped) => {
                let (motion, map) = mapped.binding();
                self.layers.bind_mapped(layer, prop, motion, map);
            }
            MotionProp::Value(value) => self.layers.set_prop(layer, prop, value, animate),
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
            self.layers.set_effect(layer, slot, Some(effect), true);
            self.mark(Dirty::Composite);
        }
    }

    pub(crate) fn set_layout_transition(&mut self, id: ElementId, transition: LayoutTransition) {
        if let Some(layer) = self.layer_id(id, Slot::Own) {
            self.layers.set_layout_transition(layer, Some(transition));
        }
    }

    pub(crate) fn set_scroll_link(&mut self, id: ElementId, key: u64, link: ScrollLinked) {
        if let Some(layer) = self.layer_id(id, Slot::Own) {
            self.layers.set_scroll_link(layer, Some((key, link)));
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
            self.layers.enter(layer, from, timing);
            self.mark(Dirty::Composite);
        }
    }

    pub(crate) fn motion(&mut self, id: MotionId, op: MotionOp) {
        let layers = &mut self.layers;
        match op {
            MotionOp::Create(value) => {
                if value.is_alive() {
                    layers.create_motion(id, value.get_untracked());
                    self.motions.insert(id, value);
                }
                return;
            }
            MotionOp::Set(value) => layers.set_motion(id, value),
            MotionOp::Animate {
                target,
                velocity,
                timing,
                done,
            } => layers.animate_motion(id, target, velocity, timing, done),
            MotionOp::Decay {
                velocity,
                decay,
                done,
            } => layers.decay_motion(id, velocity, decay, done),
            MotionOp::Stop => layers.stop_motion(id),
            MotionOp::Remove => {
                layers.remove_motion(id);
                self.motions.remove(&id);
            }
        }
        self.mark(Dirty::Composite);
    }
}
