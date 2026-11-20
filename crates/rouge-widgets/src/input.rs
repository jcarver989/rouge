use crate::tokens::{HAIRLINE, Space};
use crate::util::{by_touch, chrome, entry, entry_frame, field_height};
use rouge_core::Role;
use rouge_core::{
    Apply, BuildCx, DelegateStyle, Editable, StyleDelta, Styled, View, component, div,
    focus_handle, view,
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
    test_id: Option<String>,
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
        test_id: None,
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

chrome!(TextInput);

impl View for TextInput {
    fn build(self, cx: &mut BuildCx<'_>) {
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
            test_id,
            style,
        } = self;
        let field = entry(value, focus)
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
        let (height, inset) = (field_height(), by_touch(Space::X3, Space::X2));
        let root = view! {
            <div test_id?=test_id flex px=inset py={Space::X1.px() - HAIRLINE} overflow_hidden
                 when:items_start=multiline when:items_center={!multiline}
                 when:min_h=(multiline && !fixed, 72.0) when:min_h=(!multiline, height)>
                {div().flex_1().min_w(0.0).child(field)}
            </div>
        };
        style.apply(entry_frame(root, focus)).build(cx);
    }
}
