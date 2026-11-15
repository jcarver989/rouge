use crate::typography::UiStyles;
use crate::util::fade_when_disabled;
use rouge_core::Role;
use rouge_core::{
    BuildCx, Color, IntoProp, Prop, Spring, StyleDelta, Styled, View, component, div, show, styled,
    text, view,
};
use rouge_reactive::Signal;
use std::rc::Rc;

pub struct Toggle {
    checked: Signal<bool>,
    disabled: Prop<bool>,
    style: StyleDelta,
}

#[component]
pub fn toggle(checked: Signal<bool>) -> Toggle {
    Toggle {
        checked,
        disabled: Prop::Static(false),
        style: StyleDelta::default(),
    }
}

impl Toggle {
    pub fn disabled<U>(mut self, disabled: impl IntoProp<bool, U>) -> Self {
        self.disabled = disabled.into_prop();
        self
    }
}

styled!(Toggle => style);

impl View for Toggle {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (checked, disabled) = (self.checked, Rc::new(self.disabled));
        let track = view! {
            <div flex_none w=36.0 h=20.0 rounded=10.0 border=1.0 border_color=Color::TRANSPARENT
                 bg=Role::Border>
                <div size=16.0 m=1.0 rounded=8.0 bg=Color::WHITE transition=Spring::snappy()
                     translate_x={move || if checked.get() { 16.0 } else { 0.0 }}/>
            </div>
        }
        .checked(|style| style.bg(Role::Accent))
        .focus_visible(|style| style.border_color(Role::Accent))
        .is_checked(checked);
        let track = fade_when_disabled(track, &disabled)
            .enabled(move || !disabled.get())
            .on_activate(move || checked.toggle())
            .layer()
            .transition(Spring::snappy());
        self.style.apply(track).build(cx);
    }
}

pub struct Radio<T> {
    selected: Signal<T>,
    value: T,
    label: Option<String>,
    disabled: Prop<bool>,
    style: StyleDelta,
}

#[component]
pub fn radio<T: Clone + PartialEq + 'static>(selected: Signal<T>, value: T) -> Radio<T> {
    Radio {
        selected,
        value,
        label: None,
        disabled: Prop::Static(false),
        style: StyleDelta::default(),
    }
}

impl<T> Radio<T> {
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn disabled<U>(mut self, disabled: impl IntoProp<bool, U>) -> Self {
        self.disabled = disabled.into_prop();
        self
    }
}

styled!(Radio<T> => style);

impl<T: Clone + PartialEq + 'static> View for Radio<T> {
    fn build(self, cx: &mut BuildCx<'_>) {
        let Radio {
            selected,
            value,
            label,
            disabled,
            style,
        } = self;
        let chosen = value.clone();
        let on = move || selected.with(|current| *current == value);
        let dot = div()
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .size(16.0)
            .rounded(8.0)
            .border(1.0)
            .bg(Role::Elevated)
            .border_color(Role::Border)
            .checked(|style| style.border_color(Role::Accent))
            .is_checked(on.clone())
            .child(view! { <show when=on><div size=8.0 rounded=4.0 bg=Role::Accent/></show> });
        let root = view! {
            <div flex items_center flex_none gap=8.0 rounded=8.0 border=1.0 border_color=Color::TRANSPARENT>
                {dot}
                {label.map(|label| view! { <text body primary>{label}</text> })}
            </div>
        }
        .focus_visible(|style| style.border_color(Role::Accent));
        let disabled = Rc::new(disabled);
        let root = fade_when_disabled(root, &disabled)
            .enabled(move || !disabled.get())
            .on_activate(move || {
                selected.set_if_changed(chosen.clone());
            });
        style.apply(root).build(cx);
    }
}
