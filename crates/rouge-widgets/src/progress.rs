use crate::icon::{Icon, IconName};
use crate::tokens::{ICON_SIZE, Radius};
use rouge_core::Role;
use rouge_core::{
    BuildCx, DelegateStyle, Div, Ease, Fill, IntoProp, Keyframes, Path, Point, StyleDelta, Styled,
    Theme, View, auto, canvas, component, div, pct, view,
};
use std::f32::consts::{FRAC_PI_2, TAU};

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

/// A ring that fills clockwise from the top; it is as large as the box it is given, an icon by default.
#[component]
pub fn progress_ring<T, U>(value: impl IntoProp<f32, T>, color: impl IntoProp<Fill, U>) -> Div {
    let (value, color, theme) = (value.into_prop(), color.into_prop(), Theme::current());
    let ring = canvas(move |painter| {
        let size = painter.size();
        let stroke = size.width.min(size.height) * RING_STROKE;
        let center = Point::new(size.width / 2.0, size.height / 2.0);
        let radius = (size.width.min(size.height) - stroke) / 2.0;
        let share = value.get().clamp(0.0, 1.0);
        painter.stroke(
            &arc(center, radius, 1.0),
            stroke,
            theme.resolve(Fill::Role(Role::Border)),
        );
        if share > 0.0 {
            painter.stroke(
                &arc(center, radius, share),
                stroke,
                theme.resolve(color.get()),
            );
        }
    })
    .size_full();
    view! { <div flex_none size=ICON_SIZE>{ring}</div> }
}

fn arc(center: Point, radius: f32, share: f32) -> Path {
    let steps = (share * ARC_STEPS).ceil().max(1.0);
    let point = |step: f32| {
        let angle = TAU * share * step / steps - FRAC_PI_2;
        (
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        )
    };
    let (x, y) = point(0.0);
    let mut path = Path::builder().move_to(x, y);
    let mut step = 1.0;
    while step <= steps {
        let (x, y) = point(step);
        path = path.line_to(x, y);
        step += 1.0;
    }
    path.build()
}

const RING_STROKE: f32 = 0.125;
const ARC_STEPS: f32 = 64.0;

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
