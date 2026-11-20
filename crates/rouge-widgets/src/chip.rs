use crate::icon::{Icon, IconName};
use crate::tokens::{Pace, Radius, Space, TINT_ALPHA};
use crate::typography::UiStyles;
use crate::util::{by_touch, chrome, fade_when_disabled, focus_ring, hit_box};
use rouge_core::{
    Apply, BuildCx, Color, DelegateStyle, Div, Fill, IntoProp, Prop, Role, StyleDelta, Styled,
    Theme, View, component, div, scroll, show, text, view,
};
use rouge_reactive::Callback;
use std::rc::Rc;

pub struct Chip {
    label: String,
    icon: Option<IconName>,
    count: Prop<usize>,
    selected: Prop<bool>,
    disabled: Prop<bool>,
    on_click: Callback,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn chip(#[prop(into)] label: String) -> Chip {
    Chip {
        label,
        icon: None,
        count: Prop::Static(0),
        selected: Prop::Static(false),
        disabled: Prop::Static(false),
        on_click: Callback::default(),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Chip {
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn count<T>(mut self, count: impl IntoProp<usize, T>) -> Self {
        self.count = count.into_prop();
        self
    }

    pub fn selected<T>(mut self, selected: impl IntoProp<bool, T>) -> Self {
        self.selected = selected.into_prop();
        self
    }

    pub fn disabled<T>(mut self, disabled: impl IntoProp<bool, T>) -> Self {
        self.disabled = disabled.into_prop();
        self
    }

    pub fn on_click(mut self, on_click: impl Into<Callback>) -> Self {
        self.on_click = on_click.into();
        self
    }
}

chrome!(Chip);

impl View for Chip {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (theme, on_click, count) = (Theme::current(), self.on_click, Rc::new(self.count));
        let (selected, disabled) = (Rc::new(self.selected), Rc::new(self.disabled));
        let on = {
            let selected = selected.clone();
            move || selected.get()
        };
        let ink = {
            let on = on.clone();
            move || {
                Fill::Role(if on() {
                    Role::Accent
                } else {
                    Role::TextSecondary
                })
            }
        };
        let shown = count.clone();
        let count = show(
            move || shown.get() > 0,
            move || {
                let count = count.clone();
                view! { <text caption no_wrap>{move || count.get().to_string()}</text> }
            },
        );
        let height = by_touch(HEIGHT_TOUCH, HEIGHT);
        let face = view! {
            <div flex flex_none items_center gap=Space::X1 h=height px=Space::X3 rounded=Radius::Control border=1.0
                 border_color=Role::Border text_color={ink.clone()}>
                {self.icon.map(|icon| Icon::new(icon, ink))}
                <text label no_wrap>{self.label}</text>
                {count}
            </div>
        }
        .hover(|style| style.bg(Role::SurfaceHover))
        .selected(move |style| {
            style
                .bg(move || theme.get().accent.with_alpha(TINT_ALPHA))
                .border_color(Color::TRANSPARENT)
        })
        .is_selected(on)
        .transition(Pace::Press);
        let root = fade_when_disabled(focus_ring(hit_box(face)), &disabled)
            .enabled(move || !disabled.get())
            .on_activate(move || on_click.call());
        self.style
            .apply(root.when_some(self.test_id, Div::test_id))
            .build(cx);
    }
}

#[component]
pub fn chip_row() -> Div {
    scroll().flex().flex_none().items_center().gap(Space::X2)
}

const HEIGHT: f32 = 28.0;
const HEIGHT_TOUCH: f32 = 32.0;
