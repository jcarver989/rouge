use crate::tokens::{BAR_HEIGHT, GROUP_HEIGHT, Pace, Radius, Space};
use crate::typography::UiStyles;
use crate::util::{Name, TestId, chrome, control_edge, field_name, labelled};
use rouge_core::{
    Apply, BuildCx, Color, Cursor, DelegateStyle, Div, Role, StyleDelta, Styled, View, component,
    div, editable, focus_handle, text, view,
};
use rouge_reactive::{Callback, Signal, effect};

pub struct CodeInput {
    value: Signal<String>,
    length: usize,
    name: Name,
    on_complete: Callback<(String,)>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn code_input(value: Signal<String>, length: usize) -> CodeInput {
    CodeInput {
        value,
        length: length.max(1),
        name: None,
        on_complete: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl CodeInput {
    pub fn on_complete(mut self, on_complete: impl Into<Callback<(String,)>>) -> Self {
        self.on_complete = on_complete.into();
        self
    }
}

chrome!(CodeInput);
labelled!(CodeInput);

impl View for CodeInput {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (value, length, on_complete) = (self.value, self.length, self.on_complete);
        let (focus, id) = (focus_handle(), self.test_id);
        effect(move || {
            let typed = value.get();
            let kept: String = typed
                .chars()
                .filter(|symbol| !symbol.is_whitespace())
                .take(length)
                .collect();
            if kept != typed {
                value.set(kept);
            } else if typed.chars().count() == length {
                on_complete.call(typed);
            }
        });
        let cells = (0..length).map(|index| {
            let symbol = move || {
                value.with(|typed| typed.chars().nth(index).map_or_default(String::from))
            };
            let active = move || {
                focus.is_focused()
                    && value.with(|typed| typed.chars().count().min(length - 1)) == index
            };
            view! {
                <div test_id?={id.part(format_args!("cell-{index}"))} access_hidden=true flex flex_none items_center justify_center w=GROUP_HEIGHT h=BAR_HEIGHT border=1.0
                     rounded=Radius::Control bg=Role::Elevated border_color={control_edge(active)}
                     translate_y={move || if active() { LIFT } else { 0.0 }} transition=Pace::Press>
                    <text code primary no_wrap>{symbol}</text>
                </div>
            }
        });
        let name = field_name(self.name, Some("Code"));
        let field = view! {
            <editable value=value w_full code color=Color::TRANSPARENT caret_color=Color::TRANSPARENT
                 selection_color=Color::TRANSPARENT track_focus=focus test_id?=id.part("input")
                 access_label?=name/>
        };
        let root = view! {
            <div relative flex flex_none items_center gap=Space::X1 cursor=Cursor::Text
                 on_pointer_down={move |_| focus.focus()}>
                {..cells}
                <div absolute inset=0.0 overflow_hidden pass_through>{field}</div>
            </div>
        };
        self.style.apply(root.when_some(id, Div::test_id)).build(cx);
    }
}

const LIFT: f32 = -2.0;
