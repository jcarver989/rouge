use crate::tokens::RADIUS_CONTROL;
use crate::typography::UiStyles;
use rouge_core::Role;
use rouge_core::{
    Apply, BuildCx, Cursor, Editable, StyleDelta, Styled, Theme, View, component, div, editable,
    focus_handle, styled, view,
};
use rouge_reactive::{Callback, Signal};
use std::ops::Range;

pub struct TextInput {
    value: Signal<String>,
    placeholder: Option<String>,
    selection: Option<Signal<Range<usize>>>,
    min_rows: Option<usize>,
    max_rows: Option<usize>,
    multiline: bool,
    submit_on_enter: bool,
    on_submit: Callback<(String,)>,
    style: StyleDelta,
}

#[component]
pub fn text_input(value: Signal<String>) -> TextInput {
    TextInput {
        value,
        placeholder: None,
        selection: None,
        min_rows: None,
        max_rows: None,
        multiline: false,
        submit_on_enter: false,
        on_submit: Callback::default(),
        style: StyleDelta::default(),
    }
}

impl TextInput {
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn selection(mut self, selection: Signal<Range<usize>>) -> Self {
        self.selection = Some(selection);
        self
    }

    pub fn min_rows(mut self, rows: usize) -> Self {
        self.min_rows = Some(rows);
        self
    }

    pub fn max_rows(mut self, rows: usize) -> Self {
        self.max_rows = Some(rows);
        self
    }

    pub fn multiline(mut self) -> Self {
        self.multiline = true;
        self
    }

    pub fn submit_on_enter(mut self) -> Self {
        self.submit_on_enter = true;
        self
    }

    pub fn on_submit(mut self, on_submit: impl Into<Callback<(String,)>>) -> Self {
        self.on_submit = on_submit.into();
        self
    }
}

styled!(TextInput => style);

impl View for TextInput {
    fn build(self, cx: &mut BuildCx<'_>) {
        let theme = Theme::current();
        let focus = focus_handle();
        let TextInput {
            value,
            placeholder,
            selection,
            min_rows,
            max_rows,
            multiline,
            submit_on_enter,
            on_submit,
            style,
        } = self;
        let field = editable(value)
            .w_full()
            .body()
            .color(Role::Text)
            .caret_color(theme.get_untracked().accent)
            .track_focus(focus)
            .submit_on_enter(submit_on_enter)
            .on_submit(move |text| on_submit.call(text.to_owned()))
            .when(multiline, Editable::multiline)
            .when_some(placeholder, |field, hint| {
                field.placeholder(hint, Role::TextMuted)
            })
            .when_some(selection, Editable::selection)
            .when_some(min_rows, Editable::min_rows)
            .when_some(max_rows, Editable::max_rows);
        let fixed = min_rows.is_some() || max_rows.is_some();
        let root = view! {
            <div flex px=8.0 py=6.0 border=1.0 rounded=RADIUS_CONTROL bg=Role::Elevated overflow_hidden
                 cursor=Cursor::Text when:items_start=multiline when:items_center={!multiline}
                 when:min_h=(multiline && !fixed, 72.0)
                 border_color={move || if focus.is_focused() { theme.get().accent } else { theme.get().border }}
                 on_pointer_down={move |_| focus.focus()}>
                {div().flex_1().min_w(0.0).child(field)}
            </div>
        };
        style.apply(root).build(cx);
    }
}
