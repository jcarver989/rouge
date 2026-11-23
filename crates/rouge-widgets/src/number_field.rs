use crate::icon::{Icon, IconName};
use crate::tokens::{CONTROL_HEIGHT_LARGE, PRESS_SCALE, Pace, Space};
use crate::typography::UiStyles;
use crate::util::{Name, TestId, chrome, entry, entry_frame, field_height, is_touch, labelled};
use rouge_core::access;
use rouge_core::{
    Apply, BuildCx, Cursor, DelegateStyle, Div, DragPhase, Editable, Key, PointerEvent, Role,
    StyleDelta, Styled, View, component, div, focus_handle, text, view,
};
use rouge_reactive::{Signal, Task, effect, on_cleanup, signal, sleep, spawn};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

pub struct NumberField {
    value: Signal<f64>,
    label: Option<String>,
    name: Name,
    range: (f64, f64),
    step: f64,
    suffix: Option<String>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn number_field(value: Signal<f64>) -> NumberField {
    NumberField {
        value,
        label: None,
        name: None,
        range: (f64::MIN, f64::MAX),
        step: 1.0,
        suffix: None,
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl NumberField {
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn range(mut self, (min, max): (f64, f64)) -> Self {
        self.range = (min, max.max(min));
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        if step > 0.0 {
            self.step = step;
        }
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }
}

chrome!(NumberField);
labelled!(NumberField);

impl View for NumberField {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (value, step, (min, max)) = (self.value, self.step, self.range);
        let focus = focus_handle();
        let places = decimals(step);
        let shown = move |number: f64| format!("{number:.places$}");
        let write = move |number: f64| {
            let rounded = shown(number.clamp(min, max)).parse().unwrap_or(number);
            value.set_if_changed(rounded);
        };
        let draft = signal(shown(value.get_untracked()));
        effect(move || {
            draft.set_if_changed(shown(value.get()));
        });
        let commit = move || {
            if let Ok(typed) = draft.get_untracked().trim().parse::<f64>() {
                write(typed);
            }
            draft.set_if_changed(shown(value.get_untracked()));
        };
        let nudge = move |steps: f64| write(value.get_untracked() + steps * step);
        let id = self.test_id;
        let touch = is_touch();
        let label_id = id.part("label");
        let named = self.label.clone();
        let label = self.label.map(|label| {
            if touch {
                view! {
                    <div test_id?=label_id flex flex_none items_center h_full pl=Space::X3 text_color=Role::TextMuted>
                        <text label no_wrap>{label}</text>
                    </div>
                }
            } else {
                scrubber(label, label_id, value, step, write)
            }
        });
        let steppers = touch.then(|| {
            [
                (IconName::Minus, "Decrease", "decrement", -1.0),
                (IconName::Plus, "Increase", "increment", 1.0),
            ]
            .map(|(icon, name, part, by)| stepper(icon, name, id.part(part), move || nudge(by)))
        });
        let field = entry(draft, focus, self.name, named.as_deref())
            .select_all_on_focus()
            .on_submit(move |_| commit())
            .on_blur(move |_| commit())
            .on_key_down(move |event| {
                match event.key {
                    Key::ArrowUp => nudge(1.0),
                    Key::ArrowDown => nudge(-1.0),
                    _ => return,
                }
                event.stop_propagation();
            })
            .when_some(id.part("input"), Editable::test_id);
        let suffix = self
            .suffix
            .map(|suffix| view! { <div flex_none pr=Space::X2><text small muted no_wrap>{suffix}</text></div> });
        let root = view! {
            <div test_id?=id flex flex_none items_center h={field_height()} min_w=120.0 overflow_hidden>
                {label}
                <div flex_1 min_w=0.0 px=Space::X2>{field}</div>
                {suffix}
                {steppers.map(|steppers| view! { <div flex flex_none h_full border_l=1.0 border_color=Role::BorderSubtle>{..steppers}</div> })}
            </div>
        };
        self.style.apply(entry_frame(root, focus)).build(cx);
    }
}

fn scrubber(
    label: String,
    id: Option<String>,
    value: Signal<f64>,
    step: f64,
    write: impl Fn(f64) + Copy + 'static,
) -> Div {
    let origin = signal(0.0);
    view! {
        <div test_id?=id flex flex_none items_center h_full px=Space::X2 text_color=Role::TextMuted
             cursor=Cursor::ResizeHorizontal
             on_drag={move |event| match event.phase {
                 DragPhase::Start => origin.set(value.get_untracked()),
                 DragPhase::Move => {
                     let steps = (f64::from(event.translation.x) / SCRUB_STEP).round();
                     write(origin.get_untracked() + steps * step);
                 }
                 DragPhase::End => {}
             }}>
            <text label no_wrap>{label}</text>
        </div>
    }
    .hover(|style| style.bg(Role::SurfaceHover).text_color(Role::Text))
}

fn stepper(
    icon: IconName,
    name: &str,
    id: Option<String>,
    nudge: impl Fn() + Copy + 'static,
) -> Div {
    let (pressed, repeated) = (signal(false), Rc::new(Cell::new(false)));
    let timer = Rc::new(Cell::new(None::<Task>));
    let stop = {
        let timer = timer.clone();
        move || {
            if let Some(task) = timer.take() {
                task.cancel();
            }
        }
    };
    on_cleanup(stop.clone());
    let held = repeated.clone();
    view! {
        <div test_id?=id role=access::Role::Button access_label={name.to_owned()} flex flex_none
             items_center justify_center w=CONTROL_HEIGHT_LARGE h_full preserve_focus
             scale={move || if pressed.get() { PRESS_SCALE } else { 1.0 }} transition=Pace::Press
             on_pointer_down=PointerEvent::stop_propagation
             on_press={move |down| {
                 pressed.set(down);
                 stop();
                 if down {
                     let repeated = repeated.clone();
                     repeated.set(false);
                     timer.set(Some(spawn(async move {
                         sleep(REPEAT_DELAY).await;
                         loop {
                             repeated.set(true);
                             nudge();
                             sleep(REPEAT_EVERY).await;
                         }
                     })));
                 }
             }}
             on_activate={move || {
                 if !held.get() {
                     nudge();
                 }
             }}>
            {Icon::new(icon, Role::TextSecondary)}
        </div>
    }
}

const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_EVERY: Duration = Duration::from_millis(80);

fn decimals(step: f64) -> usize {
    (0..=6)
        .find(|places| {
            let scaled = step * 10f64.powi(*places);
            (scaled - scaled.round()).abs() < 1e-9
        })
        .map_or(6, |places| places.unsigned_abs() as usize)
}

const SCRUB_STEP: f64 = 4.0;
