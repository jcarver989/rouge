use super::model::{LayerId, LayerModel, MotionId};
use super::{Commit, Content, ListDelta, Nested};
use crate::animation::{Animated, Decay, Timing};
use crate::motion::Completion;
use std::mem::take;
use std::ops::{Deref, DerefMut};

#[derive(Default)]
pub struct Layers {
    model: LayerModel,
    content: Vec<(LayerId, ListDelta)>,
    waiting: Vec<(MotionId, Completion)>,
}

impl Deref for Layers {
    type Target = LayerModel;

    fn deref(&self) -> &LayerModel {
        &self.model
    }
}

impl DerefMut for Layers {
    fn deref_mut(&mut self) -> &mut LayerModel {
        &mut self.model
    }
}

impl Layers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn completion_deadline(&self) -> Option<f64> {
        self.waiting
            .iter()
            .filter_map(|(id, _)| self.model.motion_end(*id))
            .reduce(f64::min)
    }

    pub fn commit(&mut self) -> Commit {
        Commit {
            layers: self.model.take_delta(),
            content: take(&mut self.content),
            ..Commit::default()
        }
    }

    pub fn advance(&mut self, time: f64) {
        self.model.advance(time);
        self.resolve(true, |model, id| !model.is_moving(id));
    }

    pub fn set_content(&mut self, id: LayerId, content: Content) {
        self.update_content(id, content.nested, ListDelta::Replace(content.lists));
    }

    pub(crate) fn update_content(&mut self, id: LayerId, nested: Vec<Nested>, delta: ListDelta) {
        self.model.set_nested(id, nested);
        if !matches!(&delta, ListDelta::Patch(patch) if patch.is_empty()) {
            self.content.push((id, delta));
        }
    }

    pub fn set_motion(&mut self, id: MotionId, value: f32) {
        self.interrupt(id);
        self.model
            .drive_motion(id, |motion, _| *motion = Animated::new(value));
    }

    pub fn stop_motion(&mut self, id: MotionId) {
        self.interrupt(id);
        self.model
            .drive_motion(id, |motion, at| *motion = Animated::new(motion.value(at)));
    }

    pub fn remove_motion(&mut self, id: MotionId) {
        self.interrupt(id);
        self.model.remove_motion(id);
    }

    pub fn animate_motion(
        &mut self,
        id: MotionId,
        target: f32,
        velocity: Option<f32>,
        timing: Timing,
        done: Completion,
    ) {
        self.drive(id, done, |motion, at| match (velocity, timing) {
            (Some(velocity), Timing::Spring(spring)) => motion.fling(velocity, target, spring, at),
            _ => motion.animate_to(target, timing, at),
        });
    }

    pub fn decay_motion(&mut self, id: MotionId, velocity: f32, decay: Decay, done: Completion) {
        self.drive(id, done, |motion, at| motion.decay(velocity, decay, at));
    }

    fn drive(&mut self, id: MotionId, done: Completion, start: impl FnOnce(&mut Animated, f64)) {
        if self.model.motion(id).is_none() {
            return;
        }
        self.interrupt(id);
        if self.model.drive_motion(id, start) {
            self.waiting.push((id, done));
        } else {
            done.complete(true);
        }
    }

    fn interrupt(&mut self, id: MotionId) {
        self.resolve(false, |_, waiting| waiting == id);
    }

    fn resolve(&mut self, completed: bool, finished: impl Fn(&LayerModel, MotionId) -> bool) {
        let model = &self.model;
        let (finished, waiting): (Vec<_>, _) = take(&mut self.waiting)
            .into_iter()
            .partition(|(id, _)| finished(model, *id));
        self.waiting = waiting;
        for (_, done) in finished {
            done.complete(completed);
        }
    }
}
