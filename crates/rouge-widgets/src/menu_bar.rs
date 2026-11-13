use crate::menu::{MenuFocus, menu_trigger};
use crate::popover::Placement;
use rouge_core::component;
use rouge_core::{Div, Menu, Styled, Theme, div};

#[component]
pub fn menu_bar(menus: Vec<Menu>) -> Div {
    let theme = Theme::current();
    div()
        .test_id("application-menu")
        .preserve_focus()
        .flex()
        .flex_none()
        .h(28.0)
        .bg(theme.elevated)
        .text_color(theme.text)
        .children(
            menus
                .into_iter()
                .map(|menu| menu_trigger(menu, Placement::Below, MenuFocus::Preserve)),
        )
}
