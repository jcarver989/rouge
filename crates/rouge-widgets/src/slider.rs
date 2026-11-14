use crate::util::focus_ring;
use rouge_core::{
    Color, Cursor, Div, Key, PointerButton, Styled, Theme, component, div, pct, view,
};
use rouge_reactive::Signal;
use std::cell::Cell;
use std::rc::Rc;

const THUMB: f32 = 16.0;
const BORDER: f32 = 1.0;

#[component]
pub fn slider(value: Signal<f32>, range: Option<(f32, f32)>, step: Option<f32>) -> Div {
    let theme = Theme::current();
    let (min, max) = range.unwrap_or((0.0, 1.0));
    let max = max.max(min + f32::EPSILON);
    let step = step.filter(|step| *step > 0.0);
    let snap = move |raw: f32| {
        let stepped = step.map_or(raw, |step| min + ((raw - min) / step).round() * step);
        stepped.clamp(min, max)
    };
    let fraction = move || ((value.get() - min) / (max - min)).clamp(0.0, 1.0);
    let place = move |x: f32, width: f32| {
        let travel = (width - THUMB).max(1.0);
        let at = ((x - THUMB / 2.0) / travel).clamp(0.0, 1.0);
        value.set_if_changed(snap(min + at * (max - min)));
    };
    let pressed = Rc::new(Cell::new((0.0, 0.0)));
    let origin = pressed.clone();
    let nudge = step.unwrap_or((max - min) / 20.0);
    let track = div()
        .absolute()
        .left(0.0)
        .right(0.0)
        .h(4.0)
        .rounded(2.0)
        .bg(theme.border());
    let fill = div()
        .h_full()
        .rounded(2.0)
        .bg(theme.accent())
        .w(move || pct(fraction() * 100.0));
    let thumb = div()
        .absolute()
        .size(THUMB)
        .rounded(THUMB / 2.0)
        .bg(Color::WHITE)
        .border(1.0)
        .border_color(theme.border())
        .left(move || pct(fraction() * 100.0));
    let rail = div()
        .absolute()
        .left(0.0)
        .right(THUMB - 2.0 * BORDER)
        .h(THUMB)
        .child(thumb);
    let root = view! {
        <div relative flex items_center h=THUMB min_w=40.0 rounded={THUMB / 2.0} border=BORDER
             border_color=Color::TRANSPARENT/>
    };
    focus_ring(root)
        .focusable()
        .cursor(Cursor::Pointer)
        .on_pointer_down(move |event| {
            if event.button == PointerButton::Primary {
                let (x, width) = (event.local().x, event.size().width);
                pressed.set((x, width));
                place(x, width);
            }
        })
        .on_drag(move |event| {
            let (x, width) = origin.get();
            place(x + event.translation.x, width);
        })
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
        .child(rail)
}
