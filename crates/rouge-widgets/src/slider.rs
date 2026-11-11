use crate::util::ring;
use rouge_core::{
    BuildCx, Color, Cursor, Div, ElementId, Key, PointerButton, Rect, Styled, Theme, View, div,
    pct, px, styled, use_theme,
};
use rouge_reactive::{Signal, signal};
use std::cell::Cell;
use std::rc::Rc;

const THUMB: f32 = 16.0;

pub struct Slider {
    root: Div,
    value: Signal<f32>,
    range: (f32, f32),
    step: Option<f32>,
    theme: Theme,
}

#[track_caller]
pub fn slider(value: Signal<f32>) -> Slider {
    let theme = use_theme();
    let root = div()
        .relative()
        .flex()
        .items_center()
        .h(px(THUMB))
        .min_w(40.0)
        .rounded(px(THUMB / 2.0));
    Slider {
        root,
        value,
        range: (0.0, 1.0),
        step: None,
        theme,
    }
}

impl Slider {
    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.range = (min, max.max(min + f32::EPSILON));
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = Some(step).filter(|step| *step > 0.0);
        self
    }
}

styled!(Slider => root);

impl View for Slider {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (value, (min, max), step, theme) = (self.value, self.range, self.step, self.theme);
        let bounds = signal(Rect::ZERO);
        let snap = move |raw: f32| {
            let stepped = step.map_or(raw, |step| min + ((raw - min) / step).round() * step);
            stepped.clamp(min, max)
        };
        let fraction = move || ((value.get() - min) / (max - min)).clamp(0.0, 1.0);
        let place = move |x: f32| {
            let travel = (bounds.get_untracked().size.width - THUMB).max(1.0);
            let at = ((x - THUMB / 2.0) / travel).clamp(0.0, 1.0);
            value.set_if_changed(snap(min + at * (max - min)));
        };
        let dragging = Rc::new(Cell::new(false));
        let (started, ended) = (dragging.clone(), dragging.clone());
        let nudge = step.unwrap_or((max - min) / 20.0);
        let track = div()
            .absolute()
            .left(0.0)
            .right(0.0)
            .h(4.0)
            .rounded(2.0)
            .bg(theme.border);
        let fill = div()
            .h_full()
            .rounded(2.0)
            .bg(theme.accent)
            .w(move || pct(fraction() * 100.0));
        let thumb = div()
            .absolute()
            .size(px(THUMB))
            .rounded(px(THUMB / 2.0))
            .bg(Color::WHITE)
            .border(1.0)
            .border_color(theme.border)
            .left(move || px((bounds.get().size.width - THUMB).max(0.0) * fraction()));
        self.root
            .track_bounds(bounds)
            .focusable()
            .cursor(Cursor::Pointer)
            .focus_visible(ring(theme))
            .on_pointer_down(move |event| {
                if event.button == PointerButton::Primary {
                    event.capture();
                    started.set(true);
                    place(event.local().x);
                }
            })
            .on_pointer_move(move |event| {
                if dragging.get() {
                    place(event.local().x);
                }
            })
            .on_pointer_up(move |_| ended.set(false))
            .on_key_down(move |event| {
                let current = value.get_untracked();
                let next = match event.key {
                    Key::ArrowRight | Key::ArrowUp => current + nudge,
                    Key::ArrowLeft | Key::ArrowDown => current - nudge,
                    Key::Home => min,
                    Key::End => max,
                    _ => return,
                };
                value.set_if_changed(snap(next));
                event.stop_propagation();
            })
            .child(track.child(fill))
            .child(thumb)
            .build(cx)
    }
}
