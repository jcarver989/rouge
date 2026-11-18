use crate::{
    tokens::{Radius, Space},
    typography::UiStyles,
    util::chrome,
};
use rouge_core::Role;
use rouge_core::{
    Apply, BuildCx, DelegateStyle, Div, Key, Prop, StyleDelta, Styled, Theme, View, component, div,
    editable, switch, text,
};
use rouge_reactive::{Callback, memo, signal};

pub struct InlineEdit {
    value: Prop<String>,
    test_id: Option<String>,
    on_commit: Callback<(String,)>,
    on_cancel: Callback,
    style: StyleDelta,
    editing: StyleDelta,
}

#[component]
pub fn inline_edit(#[prop(reactive)] value: Prop<String>) -> InlineEdit {
    InlineEdit {
        value,
        test_id: None,
        on_commit: Callback::default(),
        on_cancel: Callback::default(),
        style: StyleDelta::default(),
        editing: StyleDelta::default()
            .px(Space::X1)
            .rounded(Radius::Inner)
            .border(1.0)
            .border_color(Role::Accent)
            .bg(Role::Canvas)
            .text_color(Role::Text)
            .strikethrough(false),
    }
}

impl InlineEdit {
    pub fn on_commit(mut self, on_commit: impl Into<Callback<(String,)>>) -> Self {
        self.on_commit = on_commit.into();
        self
    }

    pub fn on_cancel(mut self, on_cancel: impl Into<Callback>) -> Self {
        self.on_cancel = on_cancel.into();
        self
    }

    pub fn editing(mut self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.editing = f(self.editing);
        self
    }
}

chrome!(InlineEdit);

impl View for InlineEdit {
    fn build(self, cx: &mut BuildCx<'_>) {
        let InlineEdit {
            value,
            test_id,
            on_commit,
            on_cancel,
            style,
            editing: editing_style,
        } = self;
        let accent = Theme::current().get_untracked().accent;
        let value = memo(move || value.get());
        let editing = signal(false);
        let finish = move |committed: Option<String>| {
            if !editing.get_untracked() {
                return;
            }
            editing.set(false);
            match committed {
                Some(text) => on_commit.call(text),
                None => on_cancel.call(),
            }
        };
        let content = switch(
            move || editing.get(),
            move |active| {
                if !active {
                    return text(move || value.get()).into_any();
                }
                let draft = signal(value.get_untracked());
                editable(draft)
                    .autofocus()
                    .select_all_on_focus()
                    .caret_color(accent)
                    .selection_color(accent.with_alpha(0.3))
                    .w_full()
                    .on_blur(move |_| finish(Some(draft.get_untracked())))
                    .on_submit(move |_| finish(Some(draft.get_untracked())))
                    .into_any()
            },
        );
        let root = div()
            .w_full()
            .min_w(0.0)
            .body()
            .on_double_click(move |_| editing.set(true))
            .on_key_down(move |event| {
                if event.key == Key::Escape && editing.get_untracked() {
                    event.stop_propagation();
                    finish(None);
                }
            })
            .when(move || editing.get(), |_| editing_style)
            .child(content)
            .when_some(test_id, Div::test_id);
        style.apply(root).build(cx);
    }
}
