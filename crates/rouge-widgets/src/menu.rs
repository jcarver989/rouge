use crate::components::list_row;
use crate::icon::{Icon, IconName};
use crate::material::{Material, MaterialExt};
use crate::popover::{Placement, popup};
use crate::roving::RovingFocus;
use crate::tokens::{MENU_WIDTH, RADIUS_CONTROL, RADIUS_INNER};
use crate::typography::UiStyles;
use crate::util::{activates, focus_ring};
use rouge_core::{
    BuildCx, Div, ElementId, Key, Menu, Point, PointerButton, Rect, Styled, Theme, View, component,
    div, pct, px, styled, text, view,
};
use rouge_reactive::{Signal, signal};
use std::rc::Rc;

pub use rouge_core::MenuItem;

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
    let hover = Theme::current().surface_hover;
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
        .hover(move |style| style.bg(hover.get_untracked()))
        .on_click(move |event| {
            event.stop_propagation();
            open.update(|open| *open = !*open);
        })
        .child(text(menu.title))
        .child(popup(open, Div::preserve_focus, move || {
            placement.place(
                render_items(&menu.items, open, focus, close.clone()),
                anchor,
            )
        }));
    (row, open)
}

fn render_items(
    items: &[MenuItem],
    open: Signal<bool>,
    focus: MenuFocus,
    close: Rc<dyn Fn()>,
) -> Div {
    let theme = Theme::current();
    let actionable = items
        .iter()
        .filter(|item| !matches!(item, MenuItem::Separator));
    let handles = RovingFocus::new(actionable.count());
    let mut index = 0;
    let rows = items.iter().cloned().map(|item| {
        if matches!(item, MenuItem::Separator) {
            return div().h(px(1.0)).my(4.0).bg(theme.border);
        }
        let current = index;
        index += 1;
        let (mut row, activate, submenu): (Div, Rc<dyn Fn()>, bool) = match item {
            MenuItem::Submenu(menu) => {
                let (row, child_open) = trigger(menu, Placement::Right, focus, Some(close.clone()));
                (row, Rc::new(move || child_open.set(true)), true)
            }
            MenuItem::Item {
                ref title,
                ref shortcut,
                ref id,
                ref icon,
                destructive,
                ..
            } => {
                let close = close.clone();
                let chosen = item.clone();
                let activate: Rc<dyn Fn()> = Rc::new(move || {
                    chosen.activate();
                    close();
                });
                let clicked = activate.clone();
                let fg = if destructive {
                    theme.destructive
                } else {
                    theme.text
                };
                let id = id.clone().unwrap_or_else(|| format!("menu-item-{title}"));
                let icon = icon.clone().map(|icon| Icon::new(icon, fg));
                let shortcut = shortcut
                    .as_ref()
                    .map(|chord| view! { <text caption muted>{chord.display()}</text> });
                let row = view! {
                    <list_row id on_click={move || clicked()}>
                        {icon}
                        <text label no_wrap color=fg flex_1>{title.clone()}</text>
                        {shortcut}
                    </list_row>
                };
                (div().rounded(RADIUS_INNER).child(row), activate, false)
            }
            MenuItem::Separator => unreachable!(),
        };
        if matches!(focus, MenuFocus::Take) {
            let handles = handles.clone();
            row = row
                .track_focus(handles.handle(current))
                .focus(move |style| style.bg(theme.surface_hover.get_untracked()))
                .on_key_down(move |event| {
                    match event.key {
                        Key::ArrowDown | Key::ArrowUp | Key::Home | Key::End => {
                            handles.step(current, &event.key);
                        }
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
        .apply(div().material(Material::Floating))
        .flex()
        .col()
        .p(4.0)
        .w(MENU_WIDTH)
        .max_w(pct(100.0))
        .children(rows)
}

pub struct ContextMenu {
    root: Div,
    items: Rc<[MenuItem]>,
    focus: MenuFocus,
    dropdown: bool,
}

#[component]
pub fn context_menu(children: impl View, items: impl IntoIterator<Item = MenuItem>) -> ContextMenu {
    ContextMenu {
        root: div().child(children),
        items: items.into_iter().collect(),
        focus: MenuFocus::Take,
        dropdown: false,
    }
}

impl ContextMenu {
    pub fn focus_policy(mut self, focus: MenuFocus) -> Self {
        self.focus = focus;
        self
    }

    pub fn dropdown(mut self) -> Self {
        self.dropdown = true;
        self.root = self.root.flex_none();
        self
    }
}

styled!(ContextMenu => root);

impl View for ContextMenu {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (items, focus, dropdown) = (self.items, self.focus, self.dropdown);
        let (open, at, anchor) = (signal(false), signal(Point::ZERO), signal(Rect::ZERO));
        let body = move || {
            let menu = render_items(&items, open, focus, Rc::new(move || open.set(false)));
            if dropdown {
                let width = move || px(anchor.get().size.width);
                Placement::Below.place(menu.min_w(width), anchor)
            } else {
                menu.absolute()
                    .left(move || px(at.get().x))
                    .top(move || px(at.get().y))
            }
        };
        let root = if dropdown {
            self.root
                .track_bounds(anchor)
                .focusable()
                .on_click(move |event| {
                    event.stop_propagation();
                    open.set(true);
                })
                .on_key_down(move |event| {
                    if matches!(event.key, Key::Enter | Key::Space | Key::ArrowDown) {
                        event.stop_propagation();
                        open.set(true);
                    }
                })
        } else {
            self.root.on_pointer_down(move |event| {
                if event.button == PointerButton::Secondary {
                    at.set(event.position);
                    open.set(true);
                    event.stop_propagation();
                }
            })
        };
        root.child(popup(open, |layer| layer, body)).build(cx)
    }
}

#[component]
#[track_caller]
pub fn select<T: Clone + PartialEq + 'static>(
    value: Signal<T>,
    options: impl IntoIterator<Item = (T, String)>,
) -> ContextMenu {
    let theme = Theme::current();
    let options: Rc<[(T, String)]> = options.into_iter().collect();
    let labels = options.clone();
    let current = move || {
        let chosen = value.get();
        labels
            .iter()
            .find(|(option, _)| *option == chosen)
            .map(|(_, label)| label.clone())
            .unwrap_or_default()
    };
    let items = options.iter().cloned().map(move |(option, label)| {
        MenuItem::callback(label, move || {
            value.set_if_changed(option.clone());
        })
    });
    let label = view! {
        <div flex flex_1 items_center justify_between gap=8.0>
            <text body primary no_wrap>{current}</text>
            {Icon::new(IconName::ChevronDown, theme.text_muted).size(14.0)}
        </div>
    };
    let mut menu = context_menu(label, items).dropdown();
    menu.root = focus_ring(menu.root)
        .flex()
        .items_center()
        .px(10.0)
        .py(6.0)
        .min_w(120.0)
        .border(px(1.0))
        .border_color(theme.border)
        .rounded(RADIUS_CONTROL)
        .bg(theme.elevated)
        .hover(move |style| style.bg(theme.surface_hover.get_untracked()));
    menu
}
