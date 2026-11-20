use crate::tokens::Pace;
use crate::util::focus_frame;
use rouge_core::{Div, DragAxis, DragPhase, Key, Motion, Point, Timing, motion};
use rouge_reactive::{Signal, effect, signal};

pub(crate) fn dragged(
    root: Div,
    axis: DragAxis,
    at: Motion,
    range: impl Fn() -> (f32, f32) + 'static,
    settle: impl Fn(f32, f32) -> Option<f32> + 'static,
) -> Div {
    let origin = signal(0.0);
    let along = move |point: Point| match axis {
        DragAxis::Horizontal => point.x,
        DragAxis::Vertical => point.y,
    };
    root.drag_axis(axis)
        .on_drag(move |event| match event.phase {
            DragPhase::Start => origin.set(at.get()),
            DragPhase::Move => {
                let (low, high) = range();
                let to = origin.get_untracked() + along(event.translation);
                let held = to.clamp(low, high.max(low));
                at.set(held + (to - held) * RESISTANCE);
            }
            DragPhase::End => {
                let velocity = along(event.velocity);
                if let Some(to) = settle(at.get() + velocity * PROJECTION, velocity) {
                    at.fling_to(to, velocity, Timing::from(Pace::Gesture));
                }
            }
        })
}

pub(crate) struct Snap {
    pub(crate) axis: DragAxis,
    pub(crate) selected: Signal<usize>,
    pub(crate) last: usize,
    pub(crate) lead: f32,
    pub(crate) reach: f32,
}

impl Snap {
    pub(crate) fn track(
        self,
        root: Div,
        stride: impl Fn() -> f32 + Copy + 'static,
        ready: impl Fn() -> bool + 'static,
    ) -> (Div, Motion) {
        let Snap {
            axis,
            selected,
            last,
            lead,
            reach,
        } = self;
        let at = motion(0.0);
        let rest = move |index: usize| lead - index as f32 * stride();
        let mut placed = false;
        effect(move || {
            let (to, ready) = (rest(selected.get().min(last)), ready());
            if placed {
                at.animate_to(to, Pace::Gesture);
            } else {
                at.set(to);
            }
            placed = ready;
        });
        let (back, forward) = match axis {
            DragAxis::Horizontal => (Key::ArrowLeft, Key::ArrowRight),
            DragAxis::Vertical => (Key::ArrowUp, Key::ArrowDown),
        };
        let root = dragged(
            root.focusable(),
            axis,
            at,
            move || (rest(last), rest(0)),
            move |projected, _| {
                let current = selected.get_untracked().min(last) as f32;
                let nearest = ((lead - projected) / stride().max(1.0)).round().clamp(
                    (current - reach).max(0.0),
                    (current + reach).min(last as f32),
                ) as usize;
                selected.set_if_changed(nearest);
                Some(rest(nearest))
            },
        )
        .on_key_down(move |event| {
            let current = selected.get_untracked().min(last);
            let to = match &event.key {
                key if *key == forward => (current + 1).min(last),
                key if *key == back => current.saturating_sub(1),
                Key::Home => 0,
                Key::End => last,
                _ => return,
            };
            selected.set_if_changed(to);
            event.stop_propagation();
        });
        (focus_frame(root), at)
    }
}

pub(crate) const PROJECTION: f32 = 0.2;
const RESISTANCE: f32 = 0.3;
