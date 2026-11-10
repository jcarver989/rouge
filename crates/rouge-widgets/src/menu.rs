use crate::popover::{Placement, panel, popup};
use crate::util::{activates, pressable, themed, tint};
use rouge_core::{
    BuildCx, Cursor, Div, ElementId, FocusHandle, Key, Path, Point, PointerButton, Rect, Styled,
    Theme, View, div, focus_handle, icon, styled, text, use_theme,
};
use rouge_reactive::{Signal, signal};
use std::rc::Rc;

const CHEVRON: &str = "M6 9 L12 15 L18 9";

#[derive(Clone)]
pub struct MenuItem {
    label: String,
    on_select: Rc<dyn Fn()>,
}

pub fn menu_item(label: impl Into<String>, on_select: impl Fn() + 'static) -> MenuItem {
    MenuItem {
        label: label.into(),
        on_select: Rc::new(on_select),
    }
}

fn menu(items: &[MenuItem], open: Signal<bool>, theme: Theme) -> Div {
    let handles: Rc<[FocusHandle]> = items.iter().map(|_| focus_handle()).collect();
    let rows = items.iter().cloned().enumerate().map(|(index, item)| {
        let (clicked, pressed) = (item.on_select.clone(), item.on_select);
        let handles = handles.clone();
        let row = div()
            .px(10.0)
            .py(5.0)
            .rounded(4.0)
            .cursor(Cursor::Pointer)
            .track_focus(handles[index])
            .hover(tint(theme.surface, theme.hover))
            .focus(tint(theme.surface, theme.hover))
            .on_click(move |_| {
                clicked();
                open.set(false);
            })
            .on_key_down(move |event| {
                let count = handles.len();
                match event.key {
                    Key::ArrowDown => handles[(index + 1) % count].focus(),
                    Key::ArrowUp => handles[(index + count - 1) % count].focus(),
                    _ if activates(event) => {
                        pressed();
                        open.set(false);
                    }
                    _ => return,
                }
                event.stop_propagation();
            })
            .child(themed(text(item.label), theme.text, theme).no_wrap());
        if index == 0 { row.autofocus() } else { row }
    });
    panel(theme)
        .flex()
        .col()
        .p(4.0)
        .min_w(120.0)
        .focus_scope()
        .children(rows)
}

pub struct ContextMenu {
    root: Div,
    items: Rc<[MenuItem]>,
}

pub fn context_menu(target: impl View, items: impl IntoIterator<Item = MenuItem>) -> ContextMenu {
    ContextMenu {
        root: div().child(target),
        items: items.into_iter().collect(),
    }
}

styled!(ContextMenu => root);

impl View for ContextMenu {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, items) = (use_theme(), self.items);
        let (open, at) = (signal(false), signal(Point::ZERO));
        let body = move || {
            let menu = menu(&items, open, theme).absolute();
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
            let menu = menu(&items, open, theme).min_w(move || anchor.get().size.width);
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
