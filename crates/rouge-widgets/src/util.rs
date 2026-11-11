use rouge_core::{BoxShadow, Color, Div, IntoProp, Key, KeyEvent, Style, Styled, Text, Theme};
use rouge_reactive::Signal;

pub(crate) fn ring(theme: Theme) -> impl Fn(Style) -> Style + 'static {
    move |style| {
        style.shadow(BoxShadow::new(0.0, 0.0, 0.0, theme.focus_ring.get_untracked()).spread(2.5))
    }
}

pub(crate) fn over(base: Color, top: Color) -> Color {
    let mix = |base: f32, top_channel: f32| top_channel * top.a + base * (1.0 - top.a);
    Color::new(
        mix(base.r, top.r),
        mix(base.g, top.g),
        mix(base.b, top.b),
        base.a.max(top.a),
    )
}

pub(crate) fn tint(base: Signal<Color>, top: Signal<Color>) -> impl Fn(Style) -> Style + 'static {
    move |style| style.bg(over(base.get_untracked(), top.get_untracked()))
}

pub(crate) fn either<T: Clone + 'static>(
    on: impl Fn() -> bool + 'static,
    yes: Signal<T>,
    no: Signal<T>,
) -> impl Fn() -> T + 'static {
    move || if on() { yes.get() } else { no.get() }
}

pub(crate) fn themed<U>(label: Text, color: impl IntoProp<Color, U>, theme: Theme) -> Text {
    label.color(color).text_size(theme.text_size)
}

pub(crate) fn activates(event: &KeyEvent) -> bool {
    matches!(event.key, Key::Enter | Key::Space)
}

pub(crate) fn pressable(root: Div, theme: Theme, on_press: impl Fn() + 'static) -> Div {
    root.focus_visible(ring(theme)).on_press(on_press)
}
