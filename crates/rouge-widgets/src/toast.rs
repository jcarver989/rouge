use crate::components::{ControlSize, button};
use crate::icon::Icon;
use crate::material::{Material, MaterialExt};
use crate::popover::POPPED;
use crate::tokens::{ICON_SIZE_SMALL, Pace, Radius, Space, TINT_ALPHA};
use crate::typography::UiStyles;
use crate::util::tone_icon;
use rouge_core::{BuildCx, Div, Size, Styled, Theme, View, div, each, overlay, pct, text, view};
use rouge_reactive::{
    Callback, Signal, Task, effect, expect_context, on_cleanup, provide_context, signal, sleep,
    spawn,
};
use rouge_theme::NoticeTone;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

pub struct Toast {
    message: String,
    detail: Option<String>,
    tone: NoticeTone,
    action: Option<(String, Callback)>,
    duration: Duration,
}

pub fn toast(message: impl Into<String>) -> Toast {
    Toast {
        message: message.into(),
        detail: None,
        tone: NoticeTone::Neutral,
        action: None,
        duration: Duration::from_secs(4),
    }
}

impl Toast {
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn tone(mut self, tone: NoticeTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn action(mut self, label: impl Into<String>, on_click: impl Into<Callback>) -> Self {
        self.action = Some((label.into(), on_click.into()));
        self
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    #[track_caller]
    pub fn show(self) {
        Toasts::current().show(self);
    }
}

#[derive(Clone)]
struct Entry {
    id: u64,
    toast: Rc<Toast>,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Clone, Copy)]
pub struct Toasts {
    entries: Signal<Vec<Entry>>,
    heights: Signal<Vec<(u64, f32)>>,
    next: Signal<u64>,
}

impl Toasts {
    pub fn install() -> Self {
        let toasts = Toasts {
            entries: signal(Vec::new()),
            heights: signal(Vec::new()),
            next: signal(0),
        };
        provide_context(toasts);
        toasts
    }

    #[track_caller]
    pub fn current() -> Self {
        expect_context()
    }

    pub fn show(self, toast: Toast) {
        let id = self.next.get_untracked();
        self.next.set(id + 1);
        self.entries.update(|entries| {
            entries.push(Entry {
                id,
                toast: Rc::new(toast),
            });
        });
    }

    pub fn len(self) -> usize {
        self.entries.with(Vec::len)
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    fn dismiss(self, id: u64) {
        self.entries
            .update(|entries| entries.retain(|entry| entry.id != id));
    }

    fn depth(self, id: u64) -> usize {
        self.entries.with(|entries| {
            entries
                .iter()
                .rev()
                .position(|entry| entry.id == id)
                .unwrap_or_default()
        })
    }

    fn height_above(self, id: u64) -> f32 {
        let newer: Vec<u64> = self.entries.with(|entries| {
            entries
                .iter()
                .rev()
                .take_while(|entry| entry.id != id)
                .map(|entry| entry.id)
                .collect()
        });
        self.heights.with(|heights| {
            heights
                .iter()
                .filter(|(held, _)| newer.contains(held))
                .map(|(_, height)| height + GAP)
                .sum()
        })
    }

    fn height_of(self, id: u64) -> f32 {
        self.heights.with(|heights| {
            heights
                .iter()
                .find(|(held, _)| *held == id)
                .map_or(0.0, |(_, height)| *height)
        })
    }

    fn measure(self, id: u64, height: Option<f32>) {
        self.heights.update(|heights| {
            heights.retain(|(held, _)| *held != id);
            heights.extend(height.map(|height| (id, height)));
        });
    }
}

impl View for Toasts {
    fn build(self, cx: &mut BuildCx<'_>) {
        let toasts = self;
        view! {
            <overlay>
                {
                    let fanned = signal(false);
                    let cards = view! {
                        <each items=toasts.entries key={|entry| entry.id} let:entry enter=(POPPED, Pace::Enter)
                             exit=(POPPED, Pace::Exit)>
                            {card(toasts, &entry.get_untracked(), fanned)}
                        </each>
                    };
                    let reach = move || {
                        let ends = toasts.entries.with(|entries| {
                            Some((entries.first()?.id, entries.last()?.id))
                        });
                        match ends {
                            Some((oldest, _)) if fanned.get() => {
                                toasts.height_above(oldest) + toasts.height_of(oldest)
                            }
                            Some((_, newest)) => toasts.height_of(newest),
                            None => 0.0,
                        }
                    };
                    view! {
                        <div test_id="toasts" absolute right=EDGE bottom=EDGE w=WIDTH max_w=pct(100.0) h=reach
                             on_hover={move |inside| fanned.set(inside)}>
                            {cards}
                        </div>
                    }
                }
            </overlay>
        }
        .build(cx);
    }
}

fn card(toasts: Toasts, entry: &Entry, fanned: Signal<bool>) -> Div {
    let (id, toast, theme) = (entry.id, entry.toast.clone(), Theme::current());
    let size = signal(Size::ZERO);
    effect(move || toasts.measure(id, Some(size.get().height)));
    let timer = Rc::new(Cell::new(None::<Task>));
    let stop = {
        let timer = timer.clone();
        move || {
            if let Some(task) = timer.take() {
                task.cancel();
            }
        }
    };
    let duration = toast.duration;
    effect({
        let stop = stop.clone();
        move || {
            stop();
            if !fanned.get() {
                timer.set(Some(spawn(async move {
                    sleep(duration).await;
                    toasts.dismiss(id);
                })));
            }
        }
    });
    on_cleanup(move || {
        stop();
        toasts.measure(id, None);
    });
    let tone = toast.tone;
    let ink = move || Theme::notice_style(tone).foreground;
    let action = toast.action.clone().map(|(label, on_click)| {
        view! {
            <button label=label control_size=ControlSize::Small test_id={format!("toast-{id}-action")}
                 on_click={move || {
                     on_click.call();
                     toasts.dismiss(id);
                 }}/>
        }
    });
    let stacked = move || toasts.depth(id).min(STACKED) as f32;
    let lift = move || {
        if fanned.get() {
            -toasts.height_above(id)
        } else {
            -stacked() * PEEK
        }
    };
    let shrink = move || {
        if fanned.get() {
            1.0
        } else {
            1.0 - stacked() * SHRINK
        }
    };
    let visible = move || {
        if fanned.get() || toasts.depth(id) < STACKED {
            1.0
        } else {
            0.0
        }
    };
    view! {
        <div test_id={format!("toast-{id}")} absolute left=0.0 right=0.0 bottom=0.0 flex items_center
             gap=Space::X2 p=Space::X3 material=Material::Glass track_size=size
             translate_y=lift scale=shrink opacity=visible transition=Pace::Move>
            <div flex flex_none items_center justify_center size=24.0 rounded=Radius::Full
                 bg={move || theme.resolve(ink()).with_alpha(TINT_ALPHA)}>
                {Icon::new(tone_icon(tone), ink).size(ICON_SIZE_SMALL)}
            </div>
            <div flex col flex_1 min_w=0.0>
                <text label primary>{toast.message.clone()}</text>
                {toast.detail.clone().map(|detail| view! { <text small secondary>{detail}</text> })}
            </div>
            {action}
        </div>
    }
}

const WIDTH: f32 = 340.0;
const EDGE: f32 = 16.0;
const GAP: f32 = 8.0;
const PEEK: f32 = 8.0;
const SHRINK: f32 = 0.04;
const STACKED: usize = 3;
