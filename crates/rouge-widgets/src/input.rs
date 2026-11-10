use crate::util::{either, themed};
use rouge_core::{
    BuildCx, Cursor, Div, Editable, ElementId, Styled, Theme, View, div, editable, focus_handle,
    show, styled, text, use_theme,
};
use rouge_reactive::Signal;

pub struct TextInput {
    root: Div,
    field: Editable,
    value: Signal<String>,
    placeholder: Option<String>,
    theme: Theme,
}

#[track_caller]
pub fn text_input(value: Signal<String>) -> TextInput {
    let theme = use_theme();
    let root = div()
        .flex()
        .items_center()
        .px(8.0)
        .py(6.0)
        .border(1.0)
        .rounded(theme.radius)
        .bg(theme.surface)
        .overflow_hidden();
    let field = editable(value)
        .w_full()
        .color(theme.text)
        .text_size(theme.text_size)
        .caret_color(theme.accent.get_untracked());
    TextInput {
        root,
        field,
        value,
        placeholder: None,
        theme,
    }
}

#[track_caller]
pub fn text_area(value: Signal<String>) -> TextInput {
    let input = text_input(value);
    TextInput {
        root: input.root.items_start().min_h(72.0),
        field: input.field.multiline(),
        ..input
    }
}

impl TextInput {
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn on_submit(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.field = self.field.on_submit(f);
        self
    }
}

styled!(TextInput => root);

impl View for TextInput {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (value, theme) = (self.value, self.theme);
        let focus = focus_handle();
        let hint = self.placeholder.map(|placeholder| {
            let hint = move || {
                let hint = themed(text(placeholder.clone()), theme.text_muted, theme).no_wrap();
                div().absolute().inset(0.0).pass_through().child(hint)
            };
            show(move || value.with(String::is_empty), hint)
        });
        let slot = div()
            .relative()
            .flex_1()
            .min_w(0.0)
            .child(self.field.track_focus(focus))
            .children(hint);
        self.root
            .cursor(Cursor::Text)
            .border_color(either(
                move || focus.is_focused(),
                theme.accent,
                theme.border,
            ))
            .on_pointer_down(move |_| focus.focus())
            .child(slot)
            .build(cx)
    }
}
