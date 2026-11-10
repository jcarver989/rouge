use crate::prop::PropFrom;
use crate::view::{Patches, send};
use rouge_reactive::{Signal, expect_context, on_cleanup, signal};
use rouge_render::{Decay, MotionCommand, MotionId, Timing};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::oneshot::{Receiver, Sender, channel};

#[derive(Clone)]
pub(crate) struct UiContext {
    patches: Patches,
    next_motion: Signal<u64>,
}

impl UiContext {
    pub(crate) fn new(patches: Patches) -> Self {
        UiContext {
            patches,
            next_motion: signal(0),
        }
    }

    pub(crate) fn patches(&self) -> Patches {
        self.patches.clone()
    }

    pub(crate) fn next_id(&self) -> u64 {
        let id = self.next_motion.get_untracked();
        self.next_motion.set(id + 1);
        id
    }
}

struct Shared {
    value: Signal<f32>,
    patches: Patches,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Motion {
    id: MotionId,
    shared: Signal<Shared>,
}

#[track_caller]
pub fn motion(initial: f32) -> Motion {
    let context = expect_context::<UiContext>();
    let (id, patches) = (MotionId(context.next_id()), context.patches());
    let value = signal(initial);
    send(&patches, move |cx| cx.tree.create_motion(id, value));
    let removed = patches.clone();
    on_cleanup(move || {
        send(&removed, move |cx| {
            cx.tree.motion_command(id, MotionCommand::Remove);
        });
    });
    Motion {
        id,
        shared: signal(Shared { value, patches }),
    }
}

impl Motion {
    pub(crate) fn id(&self) -> MotionId {
        self.id
    }

    fn command(&self, command: MotionCommand, publish: Option<f32>) {
        if !self.shared.is_alive() {
            return;
        }
        let id = self.id;
        self.shared.with_untracked(|shared| {
            if let Some(value) = publish {
                shared.value.set(value);
            }
            send(&shared.patches, move |cx| {
                cx.tree.motion_command(id, command);
            });
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
        self.command(MotionCommand::Set(value), Some(value));
    }

    fn settle(&self, command: impl FnOnce(Option<Sender<bool>>) -> MotionCommand) -> Animation {
        let (done, settled) = channel();
        self.command(command(Some(done)), None);
        Animation(settled)
    }

    pub fn animate_to(&self, target: f32, timing: impl Into<Timing>) -> Animation {
        let timing = timing.into();
        self.settle(|done| MotionCommand::Animate {
            target,
            timing,
            done,
        })
    }

    pub fn decay(&self, velocity: f32, decay: Decay) -> Animation {
        self.settle(|done| MotionCommand::Decay {
            velocity,
            decay,
            done,
        })
    }

    pub fn stop(&self) {
        self.command(MotionCommand::Stop, None);
    }
}

pub struct Animation(Receiver<bool>);

impl Future for Animation {
    type Output = bool;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        Pin::new(&mut self.0)
            .poll(cx)
            .map(|completed| completed.unwrap_or(false))
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
