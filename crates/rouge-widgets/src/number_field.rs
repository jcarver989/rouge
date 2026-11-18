use crate::tokens::{GROUP_HEIGHT, Space};
use crate::typography::UiStyles;
use crate::util::{TestId, chrome, entry, entry_frame};
use rouge_core::{
    Apply, BuildCx, Cursor, DelegateStyle, DragPhase, Editable, Key, Role, StyleDelta, Styled,
    View, component, div, focus_handle, text, view,
};
use rouge_reactive::{Signal, effect, signal};

pub struct NumberField {
    value: Signal<f64>,
    label: Option<String>,
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
        let origin = signal(0.0);
        let id = self.test_id;
        let label = self.label.map(|label| {
            view! {
                <div test_id?={id.part("label")} flex flex_none items_center h_full px=Space::X2 text_color=Role::TextMuted
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
        });
        let field = entry(draft, focus)
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
            <div test_id?=id flex flex_none items_center h=GROUP_HEIGHT min_w=120.0 overflow_hidden>
                {label}
                <div flex_1 min_w=0.0 px=Space::X2>{field}</div>
                {suffix}
            </div>
        };
        self.style.apply(entry_frame(root, focus)).build(cx);
    }
}

fn decimals(step: f64) -> usize {
    (0..=6)
        .find(|places| {
            let scaled = step * 10f64.powi(*places);
            (scaled - scaled.round()).abs() < 1e-9
        })
        .map_or(6, |places| places.unsigned_abs() as usize)
}

const SCRUB_STEP: f64 = 4.0;
