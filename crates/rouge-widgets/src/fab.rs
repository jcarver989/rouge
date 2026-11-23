use crate::components::RowAction;
use crate::dock::Dock;
use crate::icon::{Icon, IconName};
use crate::material::{Material, MaterialExt};
use crate::popover::{FADED, Hint, dismissable, floating};
use crate::tokens::{ICON_SIZE_LARGE, Radius, Space};
use crate::typography::UiStyles;
use crate::util::{TestId, chrome, pressable};
use rouge_core::{
    BoxShadow, BuildCx, DelegateStyle, Div, Rect, Role, StyleDelta, Styled, View, component, div,
    text, view, window_size,
};
use rouge_reactive::{Callback, Signal, signal};
use std::rc::Rc;

pub struct Fab {
    icon: IconName,
    label: String,
    on_click: Callback,
    actions: Vec<RowAction>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn fab(icon: IconName, #[prop(into)] label: String) -> Fab {
    Fab {
        icon,
        label,
        on_click: Callback::default(),
        actions: Vec::new(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Fab {
    pub fn on_click(mut self, on_click: impl Into<Callback>) -> Self {
        self.on_click = on_click.into();
        self
    }

    pub fn action(mut self, action: RowAction) -> Self {
        self.actions.push(action);
        self
    }
}

chrome!(Fab);

fn row(action: RowAction, open: Signal<bool>) -> Div {
    let on_select = action.on_select;
    view! {
        <div test_id?=action.test_id flex flex_none items_center gap=Space::X3 pr=Space::X1
             on_activate={move || {
                 open.set(false);
                 on_select.call();
             }}>
            <div flex flex_none items_center h=32.0 px=Space::X3 material=Material::GlassControl>
                <text label primary no_wrap>{action.label}</text>
            </div>
            <div flex flex_none items_center justify_center size=MINI rounded=Radius::Full bg=Role::Elevated
                 shadow=BoxShadow::md()>
                {action.icon.map(|icon| Icon::new(icon, Role::Text).size(ICON_SIZE_LARGE))}
            </div>
        </div>
    }
}

fn disc(icon: IconName, id: Option<String>, action: impl Fn() + 'static) -> Div {
    let disc = view! {
        <div test_id?=id flex flex_none items_center justify_center size=SIZE rounded=Radius::Full
             material=Material::Pigment shadow=BoxShadow::md()>
            {Icon::new(icon, Role::AccentForeground).size(GLYPH)}
        </div>
    };
    pressable(disc).on_activate(action)
}

impl View for Fab {
    fn build(self, cx: &mut BuildCx<'_>) {
        let clearance = Dock::current().clearance();
        let (open, at, window) = (signal(false), signal(Rect::ZERO), window_size());
        let (on_click, dials) = (self.on_click, !self.actions.is_empty());
        let button = disc(self.icon, self.test_id.clone(), move || {
            if dials {
                open.set(true);
            } else {
                on_click.call();
            }
        })
        .track_bounds(at);
        let (actions, id): (Rc<[RowAction]>, _) = (self.actions.into(), self.test_id);
        let dial = move || {
            let rows: Vec<Div> = actions
                .iter()
                .cloned()
                .map(|action| row(action, open))
                .collect();
            let close =
                disc(IconName::X, id.part("close"), move || open.set(false)).access_label("Close");
            let stack = view! {
                <div test_id?=id.part("dial") absolute flex col items_end gap=Space::X3 focus_scope autofocus
                     right={move || window.get().width - at.get().max().x}
                     bottom={move || window.get().height - at.get().max().y}>
                    {..rows}
                    {close}
                </div>
            };
            dismissable(div().bg(Role::Scrim), stack, move || open.set(false))
        };
        let dial = dials.then(|| floating(move || open.get(), FADED, dial));
        let root = view! {
            <div absolute right=Space::X4 bottom={move || clearance() + Space::X4.px()}>
                {button.access_label(self.label.clone()).tooltip(self.label)}
                {dial}
            </div>
        };
        self.style.apply(root).build(cx);
    }
}

const SIZE: f32 = 56.0;
const GLYPH: f32 = 24.0;
const MINI: f32 = 48.0;
