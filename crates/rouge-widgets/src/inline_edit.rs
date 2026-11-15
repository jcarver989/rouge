use crate::{tokens::Radius, typography::UiStyles};
use rouge_core::Role;
use rouge_core::{
    Apply, BuildCx, Editable, Key, StyleDelta, Styled, Theme, View, component, div, editable,
    styled, view,
};
use rouge_reactive::{Callback, Signal, stored};
use std::mem::replace;

pub struct InlineEdit {
    value: Signal<String>,
    test_id: Option<String>,
    on_commit: Callback<(String,)>,
    on_cancel: Callback,
    style: StyleDelta,
}

#[component]
pub fn inline_edit(value: Signal<String>) -> InlineEdit {
    InlineEdit {
        value,
        test_id: None,
        on_commit: Callback::default(),
        on_cancel: Callback::default(),
        style: StyleDelta::default(),
    }
}

impl InlineEdit {
    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }

    pub fn on_commit(mut self, on_commit: impl Into<Callback<(String,)>>) -> Self {
        self.on_commit = on_commit.into();
        self
    }

    pub fn on_cancel(mut self, on_cancel: impl Into<Callback>) -> Self {
        self.on_cancel = on_cancel.into();
        self
    }
}

styled!(InlineEdit => style);

impl View for InlineEdit {
    fn build(self, cx: &mut BuildCx<'_>) {
        let InlineEdit {
            value,
            test_id,
            on_commit,
            on_cancel,
            style,
        } = self;
        let accent = Theme::current().get_untracked().accent;
        let settled = stored(false);
        let commit = move || {
            if !settled.update(|settled| replace(settled, true)) {
                on_commit.call(value.get_untracked());
            }
        };
        let field = editable(value)
            .autofocus()
            .select_all_on_focus()
            .caret_color(accent)
            .selection_color(accent.with_alpha(0.3))
            .w_full()
            .on_blur(move |_| commit())
            .on_submit(move |_| commit())
            .when_some(test_id, Editable::test_id);
        let root = view! {
            <div w_full min_w=0.0 px=4.0 rounded=Radius::Inner border=1.0 border_color=Role::Accent bg=Role::Canvas
                 body text_color=Role::Text
                 on_key_down={move |event| {
                     if event.key == Key::Escape {
                         event.stop_propagation();
                         settled.set(true);
                         on_cancel.call();
                     }
                 }}>
                {field}
            </div>
        };
        style.apply(root).build(cx);
    }
}
