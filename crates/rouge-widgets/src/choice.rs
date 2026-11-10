use crate::util::{either, pressable, themed};
use rouge_core::{
    BuildCx, Color, Div, ElementId, IntoProp, Path, Spring, Styled, Text, Theme, View, div, icon,
    show, styled, text, use_theme,
};
use rouge_reactive::Signal;
use std::rc::Rc;

const CHECK: &str = "M5 12.5 L10 17.5 L19 7";

#[track_caller]
pub fn toggle(checked: Signal<bool>) -> Div {
    let theme = use_theme();
    let knob = div()
        .size(16.0)
        .m(2.0)
        .rounded(8.0)
        .bg(Color::WHITE)
        .translate_x(move || if checked.get() { 16.0 } else { 0.0 })
        .transition(Spring::snappy());
    let track = div().flex_none().w(36.0).h(20.0).rounded(10.0);
    pressable(track, theme, move || checked.update(|on| *on = !*on))
        .bg(either(move || checked.get(), theme.accent, theme.border))
        .layer()
        .transition(Spring::snappy())
        .child(knob)
}

pub struct Choice {
    root: Div,
    on: Rc<dyn Fn() -> bool>,
    press: Box<dyn Fn()>,
    round: bool,
    label: Option<Text>,
    theme: Theme,
}

#[track_caller]
pub fn checkbox(checked: Signal<bool>) -> Choice {
    let flip = move || checked.update(|on| *on = !*on);
    choice(move || checked.get(), flip, false)
}

#[track_caller]
pub fn radio<T: Clone + PartialEq + 'static>(selected: Signal<T>, value: T) -> Choice {
    let chosen = value.clone();
    let pick = move || {
        selected.set_if_changed(chosen.clone());
    };
    choice(
        move || selected.with(|current| *current == value),
        pick,
        true,
    )
}

impl Choice {
    pub fn label<U>(mut self, label: impl IntoProp<String, U>) -> Self {
        self.label = Some(text(label));
        self
    }
}

styled!(Choice => root);

impl View for Choice {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, round) = (self.theme, self.round);
        let (lit, filled, shown) = (self.on.clone(), self.on.clone(), self.on);
        let mark = move || {
            if round {
                div().size(8.0).rounded(4.0).bg(theme.accent).into_any()
            } else {
                let check = icon(Path::svg(CHECK)).stroke(2.5).color(theme.accent_text);
                check.size(14.0).into_any()
            }
        };
        let frame = div()
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .size(16.0)
            .rounded(radius(round))
            .border(1.0)
            .border_color(either(move || lit(), theme.accent, theme.border))
            .bg(either(
                move || !round && filled(),
                theme.accent,
                theme.surface,
            ))
            .child(show(move || shown(), mark));
        let caption = self.label.map(|label| themed(label, theme.text, theme));
        pressable(self.root, theme, self.press)
            .child(frame)
            .children(caption)
            .build(cx)
    }
}

#[track_caller]
fn choice(on: impl Fn() -> bool + 'static, press: impl Fn() + 'static, round: bool) -> Choice {
    let theme = use_theme();
    let root = div()
        .flex()
        .items_center()
        .flex_none()
        .gap(theme.spacing)
        .rounded(radius(round));
    Choice {
        root,
        on: Rc::new(on),
        press: Box::new(press),
        round,
        label: None,
        theme,
    }
}

fn radius(round: bool) -> f32 {
    if round { 8.0 } else { 4.0 }
}
