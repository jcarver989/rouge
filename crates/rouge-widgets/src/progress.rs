use crate::icon::{Icon, IconName};
use crate::tokens::Radius;
use rouge_core::Role;
use rouge_core::{
    BuildCx, Div, Ease, IntoProp, Keyframes, StyleDelta, Styled, View, auto, component, div, pct,
    styled, view,
};

pub struct Separator {
    vertical: bool,
    style: StyleDelta,
}

#[component]
pub fn separator() -> Separator {
    Separator {
        vertical: false,
        style: StyleDelta::default(),
    }
}

impl Separator {
    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }
}

styled!(Separator => style);

impl View for Separator {
    fn build(self, cx: &mut BuildCx<'_>) {
        let line = div().flex_none().self_stretch().bg(Role::Border);
        let line = if self.vertical {
            line.h(auto()).w(1.0)
        } else {
            line.h(1.0)
        };
        self.style.apply(line).build(cx);
    }
}

#[component]
pub fn progress<U>(value: impl IntoProp<f32, U>) -> Div {
    let value = value.into_prop();
    view! {
        <div h=4.0 min_w=40.0 rounded=Radius::Full bg=Role::Border overflow_hidden>
            <div h_full bg=Role::Accent w={move || pct(value.get().clamp(0.0, 1.0) * 100.0)}/>
        </div>
    }
}

#[component]
pub fn spinner() -> Div {
    let spin = Keyframes::new()
        .at(0.0, |pose| pose.rotate(0.0))
        .at(1.0, |pose| pose.rotate(360.0))
        .duration(0.8)
        .ease(Ease::Linear)
        .repeat();
    view! {
        <div flex_none size=20.0 animate=spin>
            {Icon::new(IconName::LoaderCircle, Role::Accent).size(20.0)}
        </div>
    }
}
