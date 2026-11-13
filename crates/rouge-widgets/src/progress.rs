use crate::icon::{Icon, IconName};
use rouge_core::{
    Div, Ease, IntoProp, Keyframes, Styled, Theme, auto, component, div, pct, px, view,
};

#[component]
#[track_caller]
pub fn separator(#[prop(default)] vertical: bool) -> Div {
    let line = div().flex_none().self_stretch().bg(Theme::current().border);
    if vertical {
        line.h(auto()).w(px(1.0))
    } else {
        line.h(px(1.0))
    }
}

#[component]
#[track_caller]
pub fn progress<U>(value: impl IntoProp<f32, U>) -> Div {
    let theme = Theme::current();
    let value = value.into_prop();
    view! {
        <div h=6.0 min_w=40.0 rounded=3.0 bg=theme.border overflow_hidden>
            <div h_full bg=theme.accent w={move || pct(value.get().clamp(0.0, 1.0) * 100.0)}/>
        </div>
    }
}

#[component]
#[track_caller]
pub fn spinner() -> Div {
    let spin = Keyframes::new()
        .at(0.0, |pose| pose.rotate(0.0))
        .at(1.0, |pose| pose.rotate(360.0))
        .duration(0.8)
        .ease(Ease::Linear)
        .repeat();
    view! {
        <div flex_none size=20.0 animate=spin>
            {Icon::new(IconName::LoaderCircle, Theme::current().accent).size(20.0)}
        </div>
    }
}
