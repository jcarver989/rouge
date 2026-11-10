use rouge_core::{
    BuildCx, Div, Ease, ElementId, IntoProp, Keyframes, Path, Styled, View, auto, div, icon, pct,
    styled, use_theme,
};

const ARC: f32 = 4.970_563;

pub struct Separator {
    root: Div,
    vertical: bool,
}

#[track_caller]
pub fn separator() -> Separator {
    Separator {
        root: div().flex_none().self_stretch().bg(use_theme().border),
        vertical: false,
    }
}

impl Separator {
    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }
}

styled!(Separator => root);

impl View for Separator {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        if self.vertical {
            self.root.h(auto()).w(1.0).build(cx)
        } else {
            self.root.h(1.0).build(cx)
        }
    }
}

#[track_caller]
pub fn progress<U>(value: impl IntoProp<f32, U>) -> Div {
    let theme = use_theme();
    let value = value.into_prop();
    let fill = div()
        .h_full()
        .bg(theme.accent)
        .w(move || pct(value.get().clamp(0.0, 1.0) * 100.0));
    div()
        .h(6.0)
        .min_w(40.0)
        .rounded(3.0)
        .bg(theme.border)
        .overflow_hidden()
        .child(fill)
}

#[track_caller]
pub fn spinner() -> Div {
    let arc = Path::builder()
        .move_to(21.0, 12.0)
        .cubic_to(21.0, 12.0 + ARC, 12.0 + ARC, 21.0, 12.0, 21.0)
        .cubic_to(12.0 - ARC, 21.0, 3.0, 12.0 + ARC, 3.0, 12.0)
        .cubic_to(3.0, 12.0 - ARC, 12.0 - ARC, 3.0, 12.0, 3.0)
        .build();
    let spin = Keyframes::new()
        .at(0.0, |pose| pose.rotate(0.0))
        .at(1.0, |pose| pose.rotate(360.0))
        .duration(0.8)
        .ease(Ease::Linear)
        .repeat();
    let arc = icon(arc).stroke(2.5).color(use_theme().accent).size_full();
    div().flex_none().size(20.0).animate(spin).child(arc)
}
