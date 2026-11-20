use crate::tokens::{
    CHECKBOX_SIZE, CHECKBOX_SIZE_LARGE, CONTROL_HEIGHT_LARGE, Pace, Radius, Space,
};
use crate::typography::UiStyles;
use crate::util::{TestId, by_touch, chrome, fade_when_disabled, focus_ring, hit_box};
use rouge_core::Role;
use rouge_core::{
    Apply, BuildCx, Color, DelegateStyle, Div, IntoProp, LayoutTransition, Prop, StyleDelta,
    Styled, View, component, div, text, view,
};
use rouge_reactive::{Signal, signal};
use std::rc::Rc;

pub struct Toggle {
    checked: Signal<bool>,
    disabled: Prop<bool>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn toggle(checked: Signal<bool>) -> Toggle {
    Toggle {
        checked,
        disabled: Prop::Static(false),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Toggle {
    pub fn disabled<T>(mut self, disabled: impl IntoProp<bool, T>) -> Self {
        self.disabled = disabled.into_prop();
        self
    }
}

chrome!(Toggle);

impl View for Toggle {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (checked, disabled) = (self.checked, Rc::new(self.disabled));
        let pressed = signal(false);
        let (track_width, thumb) = by_touch(TOUCH, POINTER);
        let (track_height, reach) = (thumb + 4.0, track_width - 4.0);
        let width = move || thumb + if pressed.get() { STRETCH } else { 0.0 };
        let travel = move || {
            if checked.get() { reach - width() } else { 0.0 }
        };
        let thumb_id = self.test_id.part("thumb");
        let track = view! {
            <div flex_none w=track_width h=track_height rounded=Radius::Full border=1.0 border_color=Color::TRANSPARENT
                 bg=Role::SurfaceSelected>
                <div test_id?=thumb_id w=width h=thumb m=1.0 rounded=Radius::Full bg=Color::WHITE
                     translate_x=travel transition=Pace::Move
                     layout_transition={LayoutTransition::new(Pace::Press).size()}/>
            </div>
        }
        .checked(|style| style.bg(Role::Accent))
        .is_checked(checked);
        let track = fade_when_disabled(focus_ring(hit_box(track)), &disabled)
            .enabled(move || !disabled.get())
            .on_press(move |down| pressed.set(down))
            .on_activate(move || checked.toggle())
            .layer()
            .transition(Pace::Press);
        self.style
            .apply(track.when_some(self.test_id, Div::test_id))
            .build(cx);
    }
}

const POINTER: (f32, f32) = (36.0, 16.0);
const TOUCH: (f32, f32) = (52.0, 28.0);
const STRETCH: f32 = 4.0;

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

impl<T> DelegateStyle for Radio<T> {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.style
    }
}

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
        let (ring, mark, height) = by_touch(
            (CHECKBOX_SIZE_LARGE, 12.0, CONTROL_HEIGHT_LARGE),
            (CHECKBOX_SIZE, 8.0, 24.0),
        );
        let dot = view! {
            <div flex items_center justify_center flex_none size=ring rounded=Radius::Full border=1.0
                 bg=Role::Elevated border_color=Role::Border
                 checked={|style| style.bg(Role::Accent).border_color(Role::Accent)} is_checked=on.clone()>
                <div size=mark rounded=Radius::Full bg=Role::AccentForeground
                     scale={{
                         let on = on.clone();
                         move || if on() { 1.0 } else { 0.0 }
                     }}
                     transition=Pace::Press/>
            </div>
        };
        let root = view! {
            <div flex items_center flex_none h=height gap=Space::X2 rounded=Radius::Control border=1.0 border_color=Color::TRANSPARENT>
                {dot}
                {label.map(|label| view! { <text body primary>{label}</text> })}
            </div>
        };
        let disabled = Rc::new(disabled);
        let root = fade_when_disabled(focus_ring(root), &disabled)
            .enabled(move || !disabled.get())
            .on_activate(move || {
                selected.set_if_changed(chosen.clone());
            });
        style.apply(root).build(cx);
    }
}
