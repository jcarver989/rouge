use crate::menu::{MenuFocus, menu_trigger};
use crate::popover::Placement;
use rouge_core::Role;
use rouge_core::component;
use rouge_core::{Div, Menu, Styled, div};

#[component]
pub fn menu_bar(menus: Vec<Menu>) -> Div {
    div()
        .test_id("application-menu")
        .preserve_focus()
        .flex()
        .flex_none()
        .h(28.0)
        .bg(Role::Elevated)
        .text_color(Role::Text)
        .children(
            menus
                .into_iter()
                .map(|menu| menu_trigger(menu, Placement::Below, MenuFocus::Preserve)),
        )
}
