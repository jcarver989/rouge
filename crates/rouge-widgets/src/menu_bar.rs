use crate::popover::{Placement, panel, popup};
use rouge_core::{Chord, Div, Menu, MenuItem, Rect, Styled, div, text, use_theme, window_handle};
use rouge_reactive::{Signal, signal};

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
                .map(|menu| menu_trigger(menu, Placement::Below)),
        )
}

fn menu_trigger(menu: Menu, placement: Placement) -> Div {
    let open = signal(false);
    let anchor = signal(Rect::ZERO);
    let theme = use_theme();
    div()
        .test_id(format!("menu-{}", menu.title))
        .preserve_focus()
        .track_bounds(anchor)
        .px(10.0)
        .py(4.0)
        .hover(move |style| style.bg(theme.hover.get_untracked()))
        .on_click(move |event| {
            event.stop_propagation();
            open.update(|open| *open = !*open);
        })
        .child(text(menu.title))
        .child(popup(open, true, Div::preserve_focus, move || {
            placement.place(menu_items(menu.items.clone(), open), anchor)
        }))
}

fn menu_items(items: Vec<MenuItem>, open: Signal<bool>) -> Div {
    let theme = use_theme();
    let window = window_handle();
    panel(theme)
        .preserve_focus()
        .flex()
        .col()
        .min_w(160.0)
        .p(4.0)
        .children(items.into_iter().map(|item| match item {
            MenuItem::Separator => div().h(1.0).my(4.0).bg(theme.border),
            MenuItem::Submenu(menu) => menu_trigger(menu, Placement::Right),
            MenuItem::Action {
                ref title,
                ref shortcut,
                ..
            } => {
                let label = title.clone();
                let shortcut = shortcut.as_ref().map_or(String::new(), Chord::display);
                div()
                    .test_id(format!("menu-item-{label}"))
                    .flex()
                    .justify_between()
                    .gap(24.0)
                    .px(10.0)
                    .py(5.0)
                    .hover(move |style| style.bg(theme.hover.get_untracked()))
                    .on_click(move |event| {
                        event.stop_propagation();
                        window.dispatch_menu_item(item.clone());
                        open.set(false);
                    })
                    .child(text(label))
                    .child(text(shortcut).color(theme.text_muted))
            }
        }))
}
