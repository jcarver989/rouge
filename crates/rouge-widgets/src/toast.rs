use crate::components::{ControlSize, button};
use crate::dock::{BAR_INSET, Dock};
use crate::icon::Icon;
use crate::material::{Material, MaterialExt};
use crate::popover::POPPED;
use crate::snap::PROJECTION;
use crate::tokens::{ICON_SIZE_SMALL, Pace, Radius, Space, TINT_ALPHA};
use crate::typography::UiStyles;
use crate::util::{by_touch, is_touch, tone_icon};
use rouge_core::{
    BuildCx, Div, DragPhase, Size, Styled, Theme, Timing, View, div, dynamic, each, motion,
    overlay, pct, presentation, safe_area, text, view,
};
use rouge_reactive::{
    Callback, Signal, Task, effect, expect_context, memo, on_cleanup, provide_context, signal,
    sleep, spawn,
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
    banner: bool,
}

pub fn toast(message: impl Into<String>) -> Toast {
    Toast {
        message: message.into(),
        detail: None,
        tone: NoticeTone::Neutral,
        action: None,
        duration: Duration::from_secs(4),
        banner: false,
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

    pub fn banner(mut self) -> Self {
        self.banner = true;
        self
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
    dock: Dock,
}

impl Toasts {
    pub fn install() -> Self {
        let toasts = Toasts {
            entries: signal(Vec::new()),
            heights: signal(Vec::new()),
            next: signal(0),
            dock: Dock::current(),
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
        self.newer(id).len()
    }

    fn height_above(self, id: u64) -> f32 {
        let newer = self.newer(id);
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

    fn lane(self, banner: bool) -> Vec<Entry> {
        self.entries.with(|entries| {
            entries
                .iter()
                .filter(|entry| entry.toast.banner == banner)
                .cloned()
                .collect()
        })
    }

    fn newer(self, id: u64) -> Vec<u64> {
        self.entries.with(|entries| {
            let Some(at) = entries.iter().position(|entry| entry.id == id) else {
                return Vec::new();
            };
            entries[at + 1..]
                .iter()
                .filter(|entry| entry.toast.banner == entries[at].toast.banner)
                .map(|entry| entry.id)
                .collect()
        })
    }
}

impl View for Toasts {
    fn build(self, cx: &mut BuildCx<'_>) {
        let toasts = self;
        let presentation = presentation();
        let handheld = memo(move || presentation.get().is_handheld());
        for banner in [false, true] {
            view! {
                <overlay>
                    <dynamic>{lane(toasts, banner, handheld.get())}</dynamic>
                </overlay>
            }
            .build(cx);
        }
    }
}

fn lane(toasts: Toasts, banner: bool, handheld: bool) -> Div {
    let (fanned, safe, clearance) = (signal(false), safe_area(), toasts.dock.clearance());
    let limit = if handheld { 1 } else { STACKED };
    let cards = view! {
        <each items={move || toasts.lane(banner)} key={|entry| entry.id} let:entry enter=(POPPED, Pace::Enter)
             exit=(POPPED, Pace::Exit)>
            {card(toasts, &entry.get_untracked(), limit, fanned)}
        </each>
    };
    let reach = move || {
        let lane = toasts.lane(banner);
        match (lane.first(), lane.last()) {
            (Some(oldest), _) if fanned.get() => {
                toasts.height_above(oldest.id) + toasts.height_of(oldest.id)
            }
            (_, Some(newest)) => toasts.height_of(newest.id),
            _ => 0.0,
        }
    };
    let id = if banner { "banners" } else { "toasts" };
    let stack = view! { <div test_id=id absolute h=reach>{cards}</div> };
    let stack = if handheld {
        stack.left(BAR_INSET).right(BAR_INSET)
    } else {
        stack
            .right(EDGE)
            .w(WIDTH)
            .max_w(pct(100.0))
            .on_hover(move |inside| fanned.set(inside))
    };
    match (banner, handheld) {
        (true, true) => stack.top(move || safe.get().top + GAP),
        (true, false) => stack.top(EDGE),
        (false, true) => stack.bottom(move || clearance() + GAP),
        (false, false) => stack.bottom(EDGE),
    }
}

fn card(toasts: Toasts, entry: &Entry, limit: usize, fanned: Signal<bool>) -> Div {
    let touch = is_touch();
    let (id, toast, theme) = (entry.id, entry.toast.clone(), Theme::current());
    let banner = toast.banner;
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
            <button label=label control_size={by_touch(ControlSize::Medium, ControlSize::Small)}
                 test_id={format!("toast-{id}-action")}
                 on_click={move || {
                     on_click.call();
                     toasts.dismiss(id);
                 }}/>
        }
    });
    let peeking = if limit > 1 { limit } else { 0 };
    let stacked = move || toasts.depth(id).min(peeking) as f32;
    let away = if banner { 1.0 } else { -1.0 };
    let lift = move || {
        if fanned.get() {
            away * toasts.height_above(id)
        } else {
            away * stacked() * PEEK
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
        if fanned.get() || toasts.depth(id) < limit {
            1.0
        } else {
            0.0
        }
    };
    let (x, y) = (motion(0.0), motion(0.0));
    let panel = view! {
        <div flex items_center gap=Space::X2 p=Space::X3 material=Material::Glass translate_x=x translate_y=y>
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
    };
    let panel = if touch {
        panel.on_drag(move |event| match event.phase {
            DragPhase::Start => {}
            DragPhase::Move => {
                x.set(event.translation.x);
                let along = event.translation.y * -away;
                y.set(
                    -away
                        * if along > 0.0 {
                            along
                        } else {
                            along * SWIPE_RESISTANCE
                        },
                );
            }
            DragPhase::End => {
                let sideways = x.get() + event.velocity.x * PROJECTION;
                let along = (y.get() + event.velocity.y * PROJECTION) * -away;
                if sideways.abs() > SWIPE_AWAY / 2.0 || along > SWIPE_AWAY / 4.0 {
                    toasts.dismiss(id);
                } else {
                    x.fling_to(0.0, event.velocity.x, Timing::from(Pace::Gesture));
                    y.fling_to(0.0, event.velocity.y, Timing::from(Pace::Gesture));
                }
            }
        })
    } else {
        panel
    };
    let card = view! {
        <div test_id={format!("toast-{id}")} absolute left=0.0 right=0.0 flex col track_size=size
             translate_y=lift scale=shrink opacity=visible transition=Pace::Move>
            {panel}
        </div>
    };
    if banner {
        card.top(0.0)
    } else {
        card.bottom(0.0)
    }
}

const WIDTH: f32 = 340.0;
const EDGE: f32 = 16.0;
const GAP: f32 = 8.0;
const PEEK: f32 = 8.0;
const SHRINK: f32 = 0.04;
const STACKED: usize = 3;
const SWIPE_AWAY: f32 = 160.0;
const SWIPE_RESISTANCE: f32 = 0.2;
