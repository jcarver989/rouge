use crate::menu::{MenuFocus, menu_trigger};
use crate::popover::Placement;
use rouge_core::component;
use rouge_core::{Div, Menu, Styled, div, use_theme};

#[component]
pub fn menu_bar(menus: Vec<Menu>) -> Div {
    let theme = use_theme();
    div()
        .test_id("application-menu")
        .preserve_focus()
        .flex()
        .flex_none()
        .h(28.0)
        .bg(theme.surface)
        .text_color(theme.text)
        .children(
            menus
                .into_iter()
                .map(|menu| menu_trigger(menu, Placement::Below, MenuFocus::Preserve)),
        )
}
