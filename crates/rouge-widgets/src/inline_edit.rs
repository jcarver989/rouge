use crate::{tokens::RADIUS_INNER, typography::UiStyles};
use rouge_core::Role;
use rouge_core::{BuildCx, Key, Styled, View, component, div, editable, view};
use rouge_reactive::{Callback, Signal};

pub struct InlineEdit {
    value: Signal<String>,
    on_commit: Callback<(String,)>,
    on_cancel: Callback,
}

#[component]
pub fn inline_edit(value: Signal<String>) -> InlineEdit {
    InlineEdit {
        value,
        on_commit: Callback::default(),
        on_cancel: Callback::default(),
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
}

impl View for InlineEdit {
    fn build(self, cx: &mut BuildCx<'_>) {
        let InlineEdit {
            value,
            on_commit,
            on_cancel,
        } = self;
        view! {
            <div w_full min_w=0.0 px=4.0 rounded=RADIUS_INNER border=1.0 border_color=Role::Accent bg=Role::Canvas
                 on_key_down={move |event| {
                     if event.key == Key::Escape {
                         event.stop_propagation();
                         on_cancel.call();
                     }
                 }}>
                <editable value test_id="inline-edit" autofocus select_all_on_focus body
                          color=Role::Text w_full on_submit={move |value| on_commit.call(value.to_owned())}/>
            </div>
        }
        .build(cx);
    }
}
