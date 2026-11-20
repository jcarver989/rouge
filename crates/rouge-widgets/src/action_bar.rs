use crate::components::{Button, RowAction, button};
use crate::dock::{BAR_INSET, Dock, Tier};
use crate::icon::Icon;
use crate::material::{Material, MaterialExt};
use crate::tokens::{BAR_HEIGHT, CONTROL_HEIGHT_LARGE, Radius, Space};
use crate::typography::UiStyles;
use crate::util::{chrome, focus_ring, pressable};
use rouge_core::{
    AnyView, Apply, BuildCx, Color, DelegateStyle, Div, Role, Size, StyleDelta, Styled, View,
    component, div, keyboard_inset, presentation, safe_area, text, view,
};
use rouge_reactive::signal;
use rouge_theme::ButtonVariant;
use std::mem::take;

pub struct ActionBar {
    actions: Vec<RowAction>,
    children: Vec<AnyView>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn action_bar() -> ActionBar {
    ActionBar {
        actions: Vec::new(),
        children: Vec::new(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl ActionBar {
    pub fn action(mut self, action: RowAction) -> Self {
        self.actions.push(action);
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.children.push(child.into_any());
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.children
            .extend(children.into_iter().map(View::into_any));
        self
    }
}

chrome!(ActionBar);

fn labelled(action: RowAction) -> AnyView {
    let variant = if action.destructive {
        ButtonVariant::Destructive
    } else {
        ButtonVariant::Ghost
    };
    button(action.label)
        .variant(variant)
        .on_click(action.on_select)
        .when_some(action.icon, Button::icon)
        .when_some(action.test_id, Button::test_id)
        .into_any()
}

fn cell(action: RowAction) -> Div {
    let ink = if action.destructive {
        Role::Destructive
    } else {
        Role::TextSecondary
    };
    let on_select = action.on_select;
    let cell = view! {
        <div test_id?=action.test_id flex col flex_1 min_w=0.0 items_center justify_center h=CONTROL_HEIGHT_LARGE
             rounded=Radius::Control border=1.0 border_color=Color::TRANSPARENT>
            {action.icon.map(|icon| Icon::new(icon, ink))}
            <text caption no_wrap color=ink>{action.label}</text>
        </div>
    };
    focus_ring(pressable(cell)).on_activate(move || on_select.call())
}

impl ActionBar {
    fn docked(self) -> Div {
        let (dock, keyboard, safe) = (Dock::current(), keyboard_inset(), safe_area());
        let covers = !self.actions.is_empty();
        if covers {
            dock.cover();
        }
        let size = signal(Size::ZERO);
        let bottom = move || {
            let keyboard = keyboard.get();
            let floor = if keyboard > 0.0 {
                keyboard
            } else {
                safe.get().bottom
            };
            dock.below(Tier::Actions).max(floor) + BAR_INSET
        };
        dock.hold(Tier::Actions, move || {
            let height = size.get().height;
            if height > 0.0 { bottom() + height } else { 0.0 }
        });
        let spacing = if covers { Space::X1 } else { Space::X2 };
        let cells = self.actions.into_iter().map(cell);
        let shared = self
            .children
            .into_iter()
            .map(|child| view! { <div flex col flex_1 min_w=0.0>{child}</div> });
        view! {
            <div test_id?=self.test_id absolute left=BAR_INSET right=BAR_INSET bottom=bottom flex items_center
                 gap=spacing p=spacing material=Material::Glass track_size=size>
                {..cells}
                {..shared}
            </div>
        }
    }
}

impl View for ActionBar {
    fn build(mut self, cx: &mut BuildCx<'_>) {
        if presentation().get_untracked().is_handheld() {
            return self.docked().build(cx);
        }
        let style = take(&mut self.style);
        let actions = self.actions.into_iter().map(labelled);
        let root = view! {
            <div test_id?=self.test_id flex items_center justify_end gap=Space::X2 min_h=BAR_HEIGHT px=Space::X3
                 bg=Role::Chrome>
                {..actions}
                {self.children}
            </div>
        };
        style.apply(root).build(cx);
    }
}
