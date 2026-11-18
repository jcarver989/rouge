use crate::icon::{Icon, IconName};
use crate::tokens::Radius;
use rouge_core::Role;
use rouge_core::{
    BuildCx, DelegateStyle, Div, Ease, Fill, IntoProp, Keyframes, StyleDelta, Styled, View, auto,
    component, div, pct, view,
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

impl DelegateStyle for Separator {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.style
    }
}

impl View for Separator {
    fn build(self, cx: &mut BuildCx<'_>) {
        let line = div().flex_none().self_stretch().border_color(Role::Border);
        let line = if self.vertical {
            line.h(auto()).border_l(1.0)
        } else {
            line.border_t(1.0)
        };
        self.style.apply(line).build(cx);
    }
}

#[component]
pub fn progress<T>(value: impl IntoProp<f32, T>) -> Div {
    let value = value.into_prop();
    view! {
        <div h=4.0 min_w=40.0 rounded=Radius::Full bg=Role::SurfaceSelected overflow_hidden>
            <div h_full bg=Role::Accent w={move || pct(value.get().clamp(0.0, 1.0) * 100.0)}/>
        </div>
    }
}

#[component]
pub fn spinner() -> Div {
    spinning(20.0, Role::Accent)
}

pub(crate) fn spinning<T>(size: f32, color: impl IntoProp<Fill, T>) -> Div {
    let spin = Keyframes::new()
        .at(0.0, |pose| pose.rotate(0.0))
        .at(1.0, |pose| pose.rotate(360.0))
        .duration(0.8)
        .ease(Ease::Linear)
        .repeat();
    view! {
        <div flex_none size=size animate=spin>
            {Icon::new(IconName::LoaderCircle, color).size(size)}
        </div>
    }
}
