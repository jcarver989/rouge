use crate::tokens::Space;
use crate::typography::UiStyles;
use crate::util::{Invalid, TestId, chrome};
use rouge_core::view;
use rouge_core::{
    AnyView, BuildCx, DelegateStyle, Fill, IntoProp, Prop, Role, StyleDelta, Styled, View,
    component, div, show, text,
};
use rouge_reactive::memo;
use std::rc::Rc;

pub struct Field {
    label: String,
    control: AnyView,
    hint: Option<String>,
    error: Prop<Option<String>>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn field(#[prop(into)] label: String, children: impl View) -> Field {
    Field {
        label,
        control: children.into_any(),
        hint: None,
        error: Prop::Static(None),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Field {
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn error<T>(mut self, error: impl IntoProp<Option<String>, T>) -> Self {
        self.error = error.into_prop();
        self
    }
}

chrome!(Field);

impl View for Field {
    fn build(self, cx: &mut BuildCx<'_>) {
        let error = Rc::new(self.error.memo());
        let invalid = memo({
            let error = error.clone();
            move || error.get().is_some()
        });
        let noted = self.hint.is_some();
        let hint = self.hint.unwrap_or_default();
        let note_id = self.test_id.part("note");
        let note = view! {
            <show when={move || noted || invalid.get()}>
                {
                    let (error, hint) = (error.clone(), hint.clone());
                    let note = view! {
                        <text small
                             color={move || {
                                 Fill::Role(if invalid.get() {
                                     Role::Destructive
                                 } else {
                                     Role::TextMuted
                                 })
                             }}>
                            {move || error.get().unwrap_or_else(|| hint.clone())}
                        </text>
                    };
                    view! { <div test_id?=note_id.clone()>{note}</div> }
                }
            </show>
        };
        let root = view! {
            <div flex col gap=Space::X1 test_id?=self.test_id>
                <text label color=Role::Text>{self.label}</text>
                <div provide=Invalid(invalid)>{self.control}</div>
                {note}
            </div>
        };
        self.style.apply(root).build(cx);
    }
}
