use crate::popover::{Placement, panel, popup};
use crate::util::{activates, pressable, themed, tint};
use rouge_core::{
    BuildCx, Cursor, Div, ElementId, FocusHandle, Key, Menu, Path, Point, PointerButton, Rect,
    Styled, Theme, View, div, focus_handle, icon, styled, text, use_theme,
};
use rouge_reactive::{Signal, signal};
use std::rc::Rc;

const CHEVRON: &str = "M6 9 L12 15 L18 9";

pub use rouge_core::MenuItem;

pub fn menu_item(label: impl Into<String>, on_select: impl Fn() + 'static) -> MenuItem {
    MenuItem::callback(label, on_select)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuFocus {
    #[default]
    Take,
    Preserve,
}

impl MenuFocus {
    fn apply(self, root: Div) -> Div {
        match self {
            Self::Take => root.focus_scope(),
            Self::Preserve => root.preserve_focus(),
        }
    }
}

pub(crate) fn menu_trigger(menu: Menu, placement: Placement, focus: MenuFocus) -> Div {
    trigger(menu, placement, focus, None).0
}

fn trigger(
    menu: Menu,
    placement: Placement,
    focus: MenuFocus,
    parent_close: Option<Rc<dyn Fn()>>,
) -> (Div, Signal<bool>) {
    let (open, anchor) = (signal(false), signal(Rect::ZERO));
    let theme = use_theme();
    let close: Rc<dyn Fn()> = Rc::new(move || {
        open.set(false);
        if let Some(close) = &parent_close {
            close();
        }
    });
    let row = div()
        .test_id(format!("menu-{}", menu.title))
        .preserve_focus()
        .track_bounds(anchor)
        .px(10.0)
        .py(4.0)
        .hover(tint(theme.surface, theme.hover))
        .on_click(move |event| {
            event.stop_propagation();
            open.update(|open| *open = !*open);
        })
        .child(text(menu.title))
        .child(popup(open, true, Div::preserve_focus, move || {
            placement.place(
                render_items(&menu.items, open, theme, focus, close.clone()),
                anchor,
            )
        }));
    (row, open)
}

pub(crate) fn menu_items(
    items: &[MenuItem],
    open: Signal<bool>,
    theme: Theme,
    focus: MenuFocus,
) -> Div {
    render_items(items, open, theme, focus, Rc::new(move || open.set(false)))
}

fn render_items(
    items: &[MenuItem],
    open: Signal<bool>,
    theme: Theme,
    focus: MenuFocus,
    close: Rc<dyn Fn()>,
) -> Div {
    let handles: Rc<[FocusHandle]> = items
        .iter()
        .filter(|item| !matches!(item, MenuItem::Separator))
        .map(|_| focus_handle())
        .collect();
    let mut index = 0;
    let rows = items.iter().cloned().map(|item| {
        if matches!(item, MenuItem::Separator) {
            return div().h(1.0).my(4.0).bg(theme.border);
        }
        let current = index;
        index += 1;
        let (mut row, activate, submenu): (Div, Rc<dyn Fn()>, bool) = match item {
            MenuItem::Submenu(menu) => {
                let (row, child_open) = trigger(menu, Placement::Right, focus, Some(close.clone()));
                (row, Rc::new(move || child_open.set(true)), true)
            }
            MenuItem::Action {
                ref title,
                ref shortcut,
                ..
            }
            | MenuItem::Callback {
                ref title,
                ref shortcut,
                ..
            } => {
                let close = close.clone();
                let chosen = item.clone();
                let activate: Rc<dyn Fn()> = Rc::new(move || {
                    chosen.activate();
                    close();
                });
                let clicked = activate.clone();
                let mut row = div()
                    .test_id(format!("menu-item-{title}"))
                    .px(10.0)
                    .py(5.0)
                    .rounded(4.0)
                    .cursor(Cursor::Pointer)
                    .hover(tint(theme.surface, theme.hover))
                    .on_click(move |event| {
                        event.stop_propagation();
                        clicked();
                    })
                    .child(themed(text(title.clone()), theme.text, theme).no_wrap());
                if let Some(shortcut) = shortcut {
                    row = row.flex().justify_between().gap(24.0).child(themed(
                        text(shortcut.display()),
                        theme.text_muted,
                        theme,
                    ));
                }
                (row, activate, false)
            }
            MenuItem::Separator => unreachable!(),
        };
        if matches!(focus, MenuFocus::Take) {
            let handles = handles.clone();
            row = row
                .track_focus(handles[current])
                .focus(tint(theme.surface, theme.hover))
                .on_key_down(move |event| {
                    let count = handles.len();
                    match event.key {
                        Key::ArrowDown => handles[(current + 1) % count].focus(),
                        Key::ArrowUp => handles[(current + count - 1) % count].focus(),
                        Key::ArrowLeft => open.set(false),
                        Key::ArrowRight if submenu => activate(),
                        _ if activates(event) => activate(),
                        _ => return,
                    }
                    event.stop_propagation();
                });
            if current == 0 {
                row = row.autofocus();
            }
        }
        row
    });
    focus
        .apply(panel(theme))
        .flex()
        .col()
        .p(4.0)
        .min_w(if matches!(focus, MenuFocus::Take) {
            120.0
        } else {
            160.0
        })
        .children(rows)
}

pub struct ContextMenu {
    root: Div,
    items: Rc<[MenuItem]>,
    focus: MenuFocus,
}

pub fn context_menu(target: impl View, items: impl IntoIterator<Item = MenuItem>) -> ContextMenu {
    ContextMenu {
        root: div().child(target),
        items: items.into_iter().collect(),
        focus: MenuFocus::Take,
    }
}

impl ContextMenu {
    pub fn focus_policy(mut self, focus: MenuFocus) -> Self {
        self.focus = focus;
        self
    }
}

styled!(ContextMenu => root);

impl View for ContextMenu {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, items, focus) = (use_theme(), self.items, self.focus);
        let (open, at) = (signal(false), signal(Point::ZERO));
        let body = move || {
            let menu = menu_items(&items, open, theme, focus).absolute();
            menu.left(move || at.get().x).top(move || at.get().y)
        };
        self.root
            .on_pointer_down(move |event| {
                if event.button == PointerButton::Secondary {
                    at.set(event.position);
                    open.set(true);
                    event.stop_propagation();
                }
            })
            .child(popup(open, true, |layer| layer, body))
            .build(cx)
    }
}

pub struct Select<T> {
    root: Div,
    value: Signal<T>,
    options: Rc<[(T, String)]>,
    theme: Theme,
}

#[track_caller]
pub fn select<T: Clone + PartialEq + 'static>(
    value: Signal<T>,
    options: impl IntoIterator<Item = (T, String)>,
) -> Select<T> {
    let theme = use_theme();
    let root = div()
        .flex()
        .items_center()
        .justify_between()
        .flex_none()
        .gap(theme.spacing)
        .px(10.0)
        .py(6.0)
        .min_w(120.0)
        .border(1.0)
        .border_color(theme.border)
        .rounded(theme.radius)
        .bg(theme.surface)
        .hover(tint(theme.surface, theme.hover));
    Select {
        root,
        value,
        options: options.into_iter().collect(),
        theme,
    }
}

styled!(Select<T> => root);

impl<T: Clone + PartialEq + 'static> View for Select<T> {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (value, options, theme) = (self.value, self.options, self.theme);
        let (open, anchor) = (signal(false), signal(Rect::ZERO));
        let labels = options.clone();
        let current = move || {
            let chosen = value.get();
            let label = labels
                .iter()
                .find(|(option, _)| *option == chosen)
                .map(|(_, label)| label.clone());
            label.unwrap_or_default()
        };
        let body = move || {
            let items: Vec<_> = options
                .iter()
                .cloned()
                .map(|(option, label)| {
                    menu_item(label, move || {
                        value.set_if_changed(option.clone());
                    })
                })
                .collect();
            let menu = menu_items(&items, open, theme, MenuFocus::Take)
                .min_w(move || anchor.get().size.width);
            Placement::Below.place(menu, anchor)
        };
        let root = self.root.track_bounds(anchor);
        pressable(root, theme, move || open.update(|open| *open = !*open))
            .on_key_down(move |event| {
                if event.key == Key::ArrowDown {
                    open.set(true);
                    event.stop_propagation();
                }
            })
            .child(themed(text(current), theme.text, theme).no_wrap())
            .child(
                icon(Path::svg(CHEVRON))
                    .stroke(2.0)
                    .color(theme.text_muted)
                    .size(14.0)
                    .flex_none(),
            )
            .child(popup(open, true, |layer| layer, body))
            .build(cx)
    }
}
