use crate::animation::{Decay, Timing};
use crate::compositor::MotionId;
use crate::prop::PropFrom;
use crate::view::{Patch, Queue};
use rouge_reactive::{Signal, expect_context, on_cleanup, signal};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

#[derive(Clone)]
pub(crate) struct UiContext {
    queue: Queue,
    next_id: Signal<u64>,
}

impl UiContext {
    pub(crate) fn new(queue: Queue) -> Self {
        UiContext {
            queue,
            next_id: signal(0),
        }
    }

    pub(crate) fn queue(&self) -> Queue {
        self.queue.clone()
    }

    pub(crate) fn next_id(&self) -> u64 {
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        id
    }
}

pub(crate) enum MotionOp {
    Create(Signal<f32>),
    Set(f32),
    Animate {
        target: f32,
        timing: Timing,
        done: Completion,
    },
    Decay {
        velocity: f32,
        decay: Decay,
        done: Completion,
    },
    Stop,
    Remove,
}

struct Shared {
    value: Signal<f32>,
    queue: Queue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Motion {
    id: MotionId,
    shared: Signal<Shared>,
}

#[track_caller]
pub fn motion(initial: f32) -> Motion {
    let context = expect_context::<UiContext>();
    let (id, queue) = (MotionId(context.next_id()), context.queue());
    let value = signal(initial);
    queue.push(Patch::Motion(id, MotionOp::Create(value)));
    let removed = queue.clone();
    on_cleanup(move || removed.push(Patch::Motion(id, MotionOp::Remove)));
    Motion {
        id,
        shared: signal(Shared { value, queue }),
    }
}

impl Motion {
    pub(crate) fn id(&self) -> MotionId {
        self.id
    }

    fn command(&self, op: MotionOp, publish: Option<f32>) {
        if !self.shared.is_alive() {
            return;
        }
        let id = self.id;
        self.shared.with_untracked(|shared| {
            if let Some(value) = publish {
                shared.value.set(value);
            }
            shared.queue.push(Patch::Motion(id, op));
        });
    }

    pub fn get(&self) -> f32 {
        if !self.shared.is_alive() {
            return 0.0;
        }
        self.shared
            .with_untracked(|shared| shared.value.get_untracked())
    }

    pub fn set(&self, value: f32) {
        self.command(MotionOp::Set(value), Some(value));
    }

    pub fn animate_to(&self, target: f32, timing: impl Into<Timing>) -> Animation {
        let (done, animation) = Animation::pending();
        let timing = timing.into();
        self.command(
            MotionOp::Animate {
                target,
                timing,
                done,
            },
            None,
        );
        animation
    }

    pub fn decay(&self, velocity: f32, decay: Decay) -> Animation {
        let (done, animation) = Animation::pending();
        self.command(
            MotionOp::Decay {
                velocity,
                decay,
                done,
            },
            None,
        );
        animation
    }

    pub fn stop(&self) {
        self.command(MotionOp::Stop, None);
    }
}

#[derive(Default)]
struct Outcome {
    completed: Option<bool>,
    waker: Option<Waker>,
}

pub struct Completion(Rc<RefCell<Outcome>>);

impl Completion {
    pub(crate) fn complete(self, completed: bool) {
        self.finish(completed);
    }

    fn finish(&self, completed: bool) {
        let waker = {
            let mut outcome = self.0.borrow_mut();
            if outcome.completed.is_some() {
                return;
            }
            outcome.completed = Some(completed);
            outcome.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        self.finish(false);
    }
}

pub struct Animation(Rc<RefCell<Outcome>>);

impl Animation {
    pub fn pending() -> (Completion, Animation) {
        let outcome = Rc::new(RefCell::new(Outcome::default()));
        (Completion(outcome.clone()), Animation(outcome))
    }
}

impl Future for Animation {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        let mut outcome = self.0.borrow_mut();
        if let Some(completed) = outcome.completed {
            return Poll::Ready(completed);
        }
        outcome.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotionProp {
    Value(f32),
    Motion(Motion),
}

impl PropFrom<f32> for MotionProp {
    fn prop_from(value: f32) -> Self {
        MotionProp::Value(value)
    }
}

impl PropFrom<Motion> for MotionProp {
    fn prop_from(value: Motion) -> Self {
        MotionProp::Motion(value)
    }
}
