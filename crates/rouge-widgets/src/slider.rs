use crate::material::{Material, MaterialExt};
use crate::popover::POPPED;
use crate::tokens::{GRAB_SCALE, Pace, Radius, Space};
use crate::typography::UiStyles;
use crate::util::{chrome, focus_ring};
use rouge_core::Role;
use rouge_core::{
    Apply, BuildCx, Color, Cursor, DelegateStyle, Div, Key, PointerButton, StyleDelta, Styled,
    View, component, div, pct, show, text, view,
};
use rouge_reactive::{Signal, signal};
use std::cell::Cell;
use std::rc::Rc;

const THUMB: f32 = 16.0;
const BORDER: f32 = 1.0;
const TIP_GAP: f32 = 4.0;

pub struct Slider {
    value: Signal<f32>,
    range: (f32, f32),
    step: Option<f32>,
    format: Option<Format>,
    test_id: Option<String>,
    style: StyleDelta,
}

type Format = Rc<dyn Fn(f32) -> String>;

#[component]
pub fn slider(value: Signal<f32>) -> Slider {
    Slider {
        value,
        range: (0.0, 1.0),
        step: None,
        format: None,
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Slider {
    pub fn range(mut self, (min, max): (f32, f32)) -> Self {
        self.range = (min, max);
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = Some(step);
        self
    }

    pub fn format(mut self, format: impl Fn(f32) -> String + 'static) -> Self {
        self.format = Some(Rc::new(format));
        self
    }
}

chrome!(Slider);

impl View for Slider {
    fn build(self, cx: &mut BuildCx<'_>) {
        let style = self.style;
        let root = slider_root(
            self.value,
            self.range,
            self.step,
            self.format,
            self.test_id.as_deref(),
        );
        style
            .apply(root.when_some(self.test_id, Div::test_id))
            .build(cx);
    }
}

fn slider_root(
    value: Signal<f32>,
    (min, max): (f32, f32),
    step: Option<f32>,
    format: Option<Format>,
    test_id: Option<&str>,
) -> Div {
    let (hovered, grabbed) = (signal(false), signal(false));
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
    let track = view! {
        <div absolute left=0.0 right=0.0 h=4.0 rounded=Radius::Full bg=Role::SurfaceSelected/>
    };
    let fill = div()
        .h_full()
        .rounded(Radius::Full)
        .bg(Role::Accent)
        .w(move || pct(fraction() * 100.0));
    let thumb = view! {
        <div absolute size=THUMB rounded=Radius::Full bg=Color::WHITE border=1.0
             border_color=Role::Border left={move || pct(fraction() * 100.0)}
             scale={move || if grabbed.get() { GRAB_SCALE } else { 1.0 }} transition=Pace::Press/>
    };
    let tip = format.map(|format| {
        let tip_id = test_id.map(|id| format!("{id}-tip"));
        let bubble = view! {
            <show when={move || hovered.get() || grabbed.get()} enter=(POPPED, Pace::Enter)
                 exit=(POPPED, Pace::Exit)>
                {
                    let format = format.clone();
                    view! {
                        <div test_id?={tip_id.clone()} flex_none px=Space::X2 py=Space::X1 material=Material::GlassControl>
                            <text caption primary no_wrap>{move || format(value.get())}</text>
                        </div>
                    }
                }
            </show>
        };
        view! {
            <div absolute pass_through flex col items_center w=THUMB bottom={THUMB + TIP_GAP}
                 left={move || pct(fraction() * 100.0)}>
                {bubble}
            </div>
        }
    });
    let rail = view! {
        <div absolute left=0.0 right={THUMB - 2.0 * BORDER} h=THUMB>
            {thumb}
            {tip}
        </div>
    };
    let root = view! {
        <div relative flex items_center h=THUMB min_w=40.0 rounded=Radius::Full border=BORDER
             border_color=Color::TRANSPARENT/>
    };
    focus_ring(root)
        .focusable()
        .cursor(Cursor::Pointer)
        .on_hover(move |inside| hovered.set(inside))
        .on_press(move |down| grabbed.set(down))
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
