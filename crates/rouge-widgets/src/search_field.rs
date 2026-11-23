use crate::components::{
    Button, ControlSize, IconButton, SegmentedControl, button, icon_button, segment,
    segmented_control,
};
use crate::icon::{Icon, IconName};
use crate::popover::POPPED;
use crate::tokens::{Pace, Space};
use crate::util::{Name, TestId, chrome, entry, entry_frame, field_height, is_touch, labelled};
use rouge_core::{
    Apply, BuildCx, DelegateStyle, Editable, Role, StyleDelta, Styled, View, component, div,
    focus_handle, show, view,
};
use rouge_reactive::{Callback, Signal, memo};
use rouge_theme::ButtonVariant;
use std::rc::Rc;

pub struct SearchField {
    value: Signal<String>,
    placeholder: Option<String>,
    name: Name,
    scopes: Option<(Signal<usize>, Rc<[String]>)>,
    on_submit: Callback<(String,)>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn search_field(value: Signal<String>) -> SearchField {
    SearchField {
        value,
        placeholder: None,
        name: None,
        scopes: None,
        on_submit: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl SearchField {
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn on_submit(mut self, on_submit: impl Into<Callback<(String,)>>) -> Self {
        self.on_submit = on_submit.into();
        self
    }

    pub fn scopes<T: Into<String>>(
        mut self,
        selected: Signal<usize>,
        labels: impl IntoIterator<Item = T>,
    ) -> Self {
        self.scopes = Some((selected, labels.into_iter().map(Into::into).collect()));
        self
    }
}

chrome!(SearchField);
labelled!(SearchField);

impl View for SearchField {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (value, focus, id, touch) = (self.value, focus_handle(), self.test_id, is_touch());
        let on_submit = self.on_submit;
        let filled = memo(move || value.with(|value| !value.is_empty()));
        let active = memo(move || focus.is_focused() || filled.get());
        let fallback = self.placeholder.as_deref().or(Some("Search"));
        let field = entry(value, focus, self.name, fallback)
            .on_submit(move |text| on_submit.call(text.to_owned()))
            .when_some(self.placeholder, |field, hint| {
                field.placeholder(hint, Role::TextMuted)
            })
            .when_some(id.part("input"), Editable::test_id);
        let clear = show(move || filled.get(), {
            let id = id.part("clear");
            move || {
                icon_button(IconName::X, "Clear".to_owned())
                    .control_size(ControlSize::Small)
                    .on_click(move || {
                        value.set(String::new());
                        focus.focus();
                    })
                    .when_some(id.clone(), IconButton::test_id)
            }
        });
        let frame = view! {
            <div test_id?=id.part("frame") flex flex_1 min_w=0.0 items_center gap=Space::X2 h={field_height()} pl=Space::X3
                 pr=Space::X2>
                {Icon::new(IconName::Search, Role::TextMuted)}
                <div flex_1 min_w=0.0>{field}</div>
                {clear}
            </div>
        };
        let cancel = touch.then(|| {
            let id = id.part("cancel");
            let cancel = move || {
                button("Cancel")
                    .variant(ButtonVariant::Ghost)
                    .on_click(move || {
                        value.set(String::new());
                        focus.blur();
                    })
                    .when_some(id.clone(), Button::test_id)
            };
            view! { <show when={move || active.get()} children=cancel enter=(POPPED, Pace::Enter)/> }
        });
        let scopes = self.scopes.map(|(selected, labels)| {
            let id = id.part("scopes");
            show(
                move || active.get(),
                move || {
                    let segments = labels.iter().cloned().enumerate().map(|(index, label)| {
                        segment(label)
                            .selected(move || selected.get() == index)
                            .on_click(move || selected.set(index))
                            .flex_1()
                    });
                    segmented_control()
                        .children(segments)
                        .when_some(id.clone(), SegmentedControl::test_id)
                },
            )
        });
        let root = view! {
            <div test_id?=id flex col gap=Space::X2>
                <div flex items_center gap=Space::X1>
                    {entry_frame(frame, focus)}
                    {cancel}
                </div>
                {scopes}
            </div>
        };
        self.style.apply(root).build(cx);
    }
}
