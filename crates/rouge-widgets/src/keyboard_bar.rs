use crate::components::{Button, IconButton, button, icon_button};
use crate::dock::{BAR_INSET, Dock, Tier};
use crate::icon::IconName;
use crate::material::{Material, MaterialExt};
use crate::tokens::{CONTROL_HEIGHT_LARGE, Space};
use crate::util::TestId;
use rouge_core::{
    AnyView, Apply, BuildCx, PointerEvent, Styled, View, auto, component, div, keyboard_inset,
    overlay, show, view,
};
use rouge_reactive::Callback;
use rouge_theme::ButtonVariant;
use std::rc::Rc;

pub struct KeyboardBar {
    on_previous: Callback,
    on_next: Callback,
    on_done: Callback,
    done_label: String,
    trailing: Option<Rc<dyn Fn() -> AnyView>>,
    test_id: Option<String>,
}

#[component]
pub fn keyboard_bar() -> KeyboardBar {
    KeyboardBar {
        on_previous: Callback::default(),
        on_next: Callback::default(),
        on_done: Callback::default(),
        done_label: "Done".to_owned(),
        trailing: None,
        test_id: None,
    }
}

impl KeyboardBar {
    pub fn on_previous(mut self, on_previous: impl Into<Callback>) -> Self {
        self.on_previous = on_previous.into();
        self
    }

    pub fn on_next(mut self, on_next: impl Into<Callback>) -> Self {
        self.on_next = on_next.into();
        self
    }

    pub fn on_done(mut self, on_done: impl Into<Callback>) -> Self {
        self.on_done = on_done.into();
        self
    }

    pub fn done_label(mut self, done_label: impl Into<String>) -> Self {
        self.done_label = done_label.into();
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }

    pub fn trailing<T: View>(mut self, trailing: impl Fn() -> T + 'static) -> Self {
        self.trailing = Some(Rc::new(move || trailing().into_any()));
        self
    }
}

impl View for KeyboardBar {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (dock, keyboard) = (Dock::current(), keyboard_inset());
        let KeyboardBar {
            on_previous,
            on_next,
            on_done,
            done_label,
            trailing,
            test_id: id,
        } = self;
        let bar = move || {
            dock.hold(Tier::Keyboard, move || {
                keyboard.get() + Space::X2.px() + CONTROL_HEIGHT_LARGE + Space::X2.px()
            });
            let step = |icon, label: &str, part: &str, action: Callback| {
                action.is_set().then(|| {
                    icon_button(icon, label.to_owned())
                        .on_click(action)
                        .when_some(id.part(part), IconButton::test_id)
                })
            };
            let finish = button(done_label.clone())
                .variant(ButtonVariant::Ghost)
                .on_click(on_done)
                .when_some(id.part("done"), Button::test_id);
            view! {
                <div test_id?={id.clone()} absolute left=BAR_INSET right=BAR_INSET
                     bottom={move || keyboard.get() + Space::X2.px()} flex items_center gap=Space::X1 p=Space::X1
                     material=Material::Glass on_pointer_down=PointerEvent::stop_propagation>
                    {step(IconName::ChevronUp, "Previous field", "previous", on_previous)}
                    {step(IconName::ChevronDown, "Next field", "next", on_next)}
                    <div flex flex_none items_center gap=Space::X1 ml=auto()>
                        {trailing.as_ref().map(|trailing| trailing())}
                        {finish}
                    </div>
                </div>
            }
        };
        overlay(move || show(move || keyboard.get() > 0.0, bar)).build(cx);
    }
}
