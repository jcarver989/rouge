use crate::components::list_row;
use crate::icon::{Icon, IconName};
use crate::kbd::kbd;
use crate::material::{Material, MaterialExt};
use crate::popover::{Placement, popup};
use crate::roving::RovingFocus;
use crate::tokens::{GROUP_HEIGHT, ICON_SIZE_SMALL, MENU_WIDTH, Radius, Space};
use crate::typography::UiStyles;
use crate::util::focus_ring;
use rouge_core::Role;
use rouge_core::{
    BuildCx, DelegateStyle, Div, Key, Menu, Point, PointerButton, Rect, Styled, View, component,
    div, pct, text, view,
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
    let hover = Role::SurfaceHover;
    let close: Rc<dyn Fn()> = Rc::new(move || {
        open.set(false);
        if let Some(close) = &parent_close {
            close();
        }
    });
    let row = view! {
        <div test_id={format!("menu-{}", menu.title)} preserve_focus track_bounds=anchor px=Space::X3
             py=Space::X1 hover={|style| style.bg(hover)}
             on_click={move |event| {
                 event.stop_propagation();
                 open.toggle();
             }}>
            <text>{menu.title}</text>
            {popup(
                open,
                Div::preserve_focus,
                move |panel| placement.place(floating_menu(panel), anchor),
                move || render_items(&menu.items, open, focus, close.clone()),
            )}
        </div>
    };
    (row, open)
}

fn render_items(
    items: &[MenuItem],
    open: Signal<bool>,
    focus: MenuFocus,
    close: Rc<dyn Fn()>,
) -> Div {
    let actionable = items
        .iter()
        .filter(|item| !matches!(item, MenuItem::Separator | MenuItem::Label(_)));
    let handles = RovingFocus::new(actionable.count());
    let mut index = 0;
    let rows = items.iter().cloned().map(|item| {
        match item {
            MenuItem::Separator => {
                return view! {
                    <div mx=Space::X2 my=Space::X1 border_t=1.0 border_color=Role::BorderSubtle/>
                };
            }
            MenuItem::Label(title) => {
                return view! { <div px=Space::X2 pt=Space::X2 pb=Space::X1><text caption muted no_wrap>{title}</text></div> };
            }
            MenuItem::Item { .. } | MenuItem::Submenu(_) => {}
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
                    Role::Destructive
                } else {
                    Role::Text
                };
                let id = id.clone().unwrap_or_else(|| format!("menu-item-{title}"));
                let icon = icon.clone().map(|icon| Icon::new(icon, fg));
                let shortcut = shortcut
                    .as_ref()
                    .map(|chord| view! { <kbd>{chord.display()}</kbd> });
                let row = view! {
                    <list_row test_id=id on_click={move || clicked()}>
                        {icon}
                        <text label no_wrap color=fg flex_1>{title.clone()}</text>
                        {shortcut}
                    </list_row>
                };
                (div().rounded(Radius::Inner).child(row), activate, false)
            }
            MenuItem::Separator | MenuItem::Label(_) => unreachable!(),
        };
        if matches!(focus, MenuFocus::Take) {
            let handles = handles.clone();
            row = row
                .track_focus(handles.handle(current))
                .focus(|style| style.bg(Role::SurfaceHover))
                .on_key_down(move |event| {
                    match event.key {
                        Key::ArrowDown | Key::ArrowUp | Key::Home | Key::End => {
                            handles.step(current, &event.key);
                        }
                        Key::ArrowLeft => open.set(false),
                        Key::ArrowRight if submenu => activate(),
                        Key::Enter | Key::Space => activate(),
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
        .apply(view! { <div/> })
        .flex()
        .col()
        .p(Space::X1)
        .children(rows)
}

fn floating_menu(panel: Div) -> Div {
    panel
        .material(Material::Glass)
        .w(MENU_WIDTH)
        .max_w(pct(100.0))
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

impl DelegateStyle for ContextMenu {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.root
    }
}

impl View for ContextMenu {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (items, focus, dropdown) = (self.items, self.focus, self.dropdown);
        let (open, at, anchor) = (signal(false), signal(Point::ZERO), signal(Rect::ZERO));
        let body = move || render_items(&items, open, focus, Rc::new(move || open.set(false)));
        let place = move |menu: Div| {
            let menu = floating_menu(menu);
            if dropdown {
                let width = move || anchor.get().size.width;
                Placement::Below.place(menu.min_w(width), anchor)
            } else {
                menu.absolute()
                    .left(move || at.get().x)
                    .top(move || at.get().y)
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
        root.child(popup(open, |layer| layer, place, body))
            .build(cx);
    }
}

#[component]
pub fn select<T: Clone + PartialEq + 'static>(
    value: Signal<T>,
    options: impl IntoIterator<Item = (T, String)>,
) -> ContextMenu {
    let options: Rc<[(T, String)]> = options.into_iter().collect();
    let labels = options.clone();
    let current = move || {
        let chosen = value.get();
        labels
            .iter()
            .find(|(option, _)| *option == chosen)
            .map_or_default(|(_, label)| label.clone())
    };
    let items = options.iter().cloned().map(move |(option, label)| {
        MenuItem::callback(label, move || {
            value.set_if_changed(option.clone());
        })
    });
    let label = view! {
        <div flex flex_1 items_center justify_between gap=Space::X2>
            <text body primary no_wrap>{current}</text>
            {Icon::new(IconName::ChevronDown, Role::TextMuted).size(ICON_SIZE_SMALL)}
        </div>
    };
    let mut menu = context_menu(label, items).dropdown();
    menu.root = focus_ring(menu.root)
        .flex()
        .items_center()
        .px(Space::X3)
        .h(GROUP_HEIGHT)
        .min_w(120.0)
        .border(1.0)
        .border_color(Role::Border)
        .rounded(Radius::Control)
        .bg(Role::Elevated)
        .hover(|style| style.bg(Role::SurfaceHover));
    menu
}
