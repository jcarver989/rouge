use crate::tokens::{ICON_SIZE, ICON_SIZE_LARGE, MotionScale, Radius};
use crate::util::chrome;
use rouge_core::Role;
use rouge_core::access;
use rouge_core::{
    BuildCx, DelegateStyle, Div, Ease, Fill, IntoProp, Keyframes, Path, Point, Prop, StyleDelta,
    Styled, Theme, View, auto, canvas, component, div, pct, view,
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
    let value = std::rc::Rc::new(value.into_prop());
    let share = value.clone();
    view! {
        <div role=access::Role::ProgressIndicator value_range=(0.0, 1.0)
             numeric_value={move || f64::from(share.get().clamp(0.0, 1.0))}
             h=4.0 min_w=40.0 rounded=Radius::Full bg=Role::SurfaceSelected overflow_hidden>
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
const ORBIT_STROKE: f32 = 1.5;
const ORBIT_SHARE: f32 = 0.25;
const ORBIT_TRACK_ALPHA: f32 = 0.18;
const ORBIT_SECONDS: f32 = 1.2;

pub struct Spinner {
    track: Prop<Fill>,
    arc: Prop<Fill>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn spinner() -> Spinner {
    Spinner {
        track: Prop::Static(Fill::Role(Role::Text)),
        arc: Prop::Static(Fill::Role(Role::Accent)),
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Spinner {
    pub fn track<T>(mut self, track: impl IntoProp<Fill, T>) -> Self {
        self.track = track.into_prop();
        self
    }

    pub fn arc<T>(mut self, arc: impl IntoProp<Fill, T>) -> Self {
        self.arc = arc.into_prop();
        self
    }
}

chrome!(Spinner);

impl View for Spinner {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (track, arc_color, theme) = (self.track, self.arc, Theme::current());
        let ring = canvas(move |painter| {
            let bounds = painter.size();
            let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
            let radius = (bounds.width.min(bounds.height) - ORBIT_STROKE) / 2.0;
            let track = theme.resolve(track.get()).with_alpha(ORBIT_TRACK_ALPHA);
            painter.stroke(&arc(center, radius, 1.0), ORBIT_STROKE, track);
            painter.stroke_round(
                &arc(center, radius, ORBIT_SHARE),
                ORBIT_STROKE,
                theme.resolve(arc_color.get()),
            );
        })
        .size_full();
        let spin = Keyframes::new()
            .at(0.0, |pose| pose.rotate(0.0))
            .at(1.0, |pose| pose.rotate(360.0))
            .duration(ORBIT_SECONDS)
            .ease(Ease::Linear)
            .repeat();
        let rotor = view! {
            <div role=access::Role::ProgressIndicator access_label="Loading"
                 flex_none size=ICON_SIZE_LARGE test_id?=self.test_id>{ring}</div>
        };
        let rotor = self.style.apply(rotor);
        if MotionScale::current() == MotionScale::Full {
            rotor.animate(spin).build(cx);
        } else {
            rotor.build(cx);
        }
    }
}
