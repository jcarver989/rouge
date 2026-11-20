use rouge_core::{keyboard_inset, safe_area, window_scope};
use rouge_reactive::{Signal, effect, on_cleanup, provide_context, signal, use_context};

pub(crate) const BAR_INSET: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tier {
    Tabs,
    Keyboard,
    Actions,
}

#[derive(Clone, Copy)]
pub(crate) struct Dock {
    held: Signal<Vec<(u64, Tier, f32)>>,
    covers: Signal<u32>,
}

impl Dock {
    fn new() -> Self {
        Dock {
            held: signal(Vec::new()),
            covers: signal(0),
        }
    }

    pub(crate) fn current() -> Self {
        use_context().unwrap_or_else(|| {
            window_scope().run(|| {
                let dock = Self::new();
                provide_context(dock);
                dock
            })
        })
    }

    pub(crate) fn hold(self, tier: Tier, top: impl Fn() -> f32 + 'static) {
        let (held, key) = (self.held, signal(()).id());
        let place = move |top: f32| {
            let mut next = held.get_untracked();
            let at = next.iter().position(|(holder, _, _)| *holder == key);
            match (at, top > 0.0) {
                (Some(at), true) => next[at].2 = top,
                (Some(at), false) => {
                    next.remove(at);
                }
                (None, true) => next.push((key, tier, top)),
                (None, false) => {}
            }
            held.set_if_changed(next);
        };
        effect(move || place(top()));
        on_cleanup(move || {
            if held.is_alive() {
                place(0.0);
            }
        });
    }

    pub(crate) fn cover(self) {
        let covers = self.covers;
        covers.update(|count| *count += 1);
        on_cleanup(move || {
            if covers.is_alive() {
                covers.update(|count| *count = count.saturating_sub(1));
            }
        });
    }

    pub(crate) fn covered(self) -> bool {
        self.covers.get() > 0
    }

    pub(crate) fn below(self, tier: Tier) -> f32 {
        self.held.with(|held| {
            held.iter()
                .filter(|(_, other, _)| *other < tier)
                .map(|(_, _, top)| *top)
                .fold(0.0, f32::max)
        })
    }

    pub(crate) fn clearance(self) -> impl Fn() -> f32 + Copy + 'static {
        let (held, keyboard, safe) = (self.held, keyboard_inset(), safe_area());
        move || {
            let top = held.with(|held| held.iter().map(|(_, _, top)| *top).fold(0.0, f32::max));
            top.max(keyboard.get()).max(safe.get().bottom)
        }
    }
}
