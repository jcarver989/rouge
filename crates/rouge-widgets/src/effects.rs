use crate::tokens::{MotionScale, Radius};
use rouge_core::view;
use rouge_core::{Color, Div, Inputs, IntoProp, Point, ShaderEffect, Styled, Theme, div, show};
use std::rc::Rc;

pub(crate) const RIPPLE_SECONDS: f32 = 0.5;
const SHIMMER_SECONDS: f32 = 1.6;
const SHIMMER_SPAN: f32 = 240.0;
const RIM_SECONDS: f32 = 1.4;
const RIM_WIDTH: f32 = 2.0;

pub(crate) struct Ripple {
    pub(crate) center: Point,
    pub(crate) ink: Color,
}

impl ShaderEffect for Ripple {
    const WGSL: &'static str = include_str!("shaders/ripple.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;
    const CLOCK: Option<usize> = Some(0);

    fn uniforms(&self, uniforms: &mut [f32; 16]) {
        uniforms[1..4].copy_from_slice(&[RIPPLE_SECONDS, self.center.x, self.center.y]);
        uniforms[4..8].copy_from_slice(&self.ink.premultiplied());
    }
}

struct Shimmer {
    glint: Color,
}

impl ShaderEffect for Shimmer {
    const WGSL: &'static str = include_str!("shaders/shimmer.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;
    const CLOCK: Option<usize> = Some(0);

    fn uniforms(&self, uniforms: &mut [f32; 16]) {
        uniforms[1..3].copy_from_slice(&[SHIMMER_SECONDS, SHIMMER_SPAN]);
        uniforms[4..8].copy_from_slice(&self.glint.premultiplied());
    }
}

struct Rim {
    light: Color,
}

impl ShaderEffect for Rim {
    const WGSL: &'static str = include_str!("shaders/rim.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;
    const CLOCK: Option<usize> = Some(0);

    fn uniforms(&self, uniforms: &mut [f32; 16]) {
        uniforms[1..3].copy_from_slice(&[RIM_SECONDS, RIM_WIDTH]);
        uniforms[4..8].copy_from_slice(&self.light.premultiplied());
    }
}

pub(crate) fn shimmering(block: Div) -> Div {
    if MotionScale::current() == MotionScale::None {
        return block;
    }
    let theme = Theme::current();
    block.effect(move || Shimmer {
        glint: theme.get().surface_selected,
    })
}

pub trait BusyExt {
    #[must_use]
    fn busy<T>(self, busy: impl IntoProp<bool, T>, radius: Radius) -> Self;
}

impl BusyExt for Div {
    fn busy<T>(self, busy: impl IntoProp<bool, T>, radius: Radius) -> Self {
        let (busy, theme) = (Rc::new(busy.into_prop()), Theme::current());
        let rim = view! {
            <show when={move || busy.get() && MotionScale::current() == MotionScale::Full}>
                {overlay(radius).test_id("busy-rim").effect(move || Rim {
                    light: theme.get().accent,
                })}
            </show>
        };
        self.relative().child(rim)
    }
}

pub(crate) fn overlay(radius: Radius) -> Div {
    view! { <div absolute inset=0.0 pass_through rounded=radius bg=Color::TRANSPARENT/> }
}
