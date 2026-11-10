mod common;

use common::builder;
use rouge_core::testing::{TestResult, assert_near, counter};
use rouge_core::{Cursor, Key, Palette, Point, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::button;

#[test]
fn a_button_fires_on_click_and_on_enter_or_space_when_focused() {
    let _entered = Runtime::new().enter();
    let (clicks, click) = counter();
    let mut ui =
        builder().build(move || div().p(10.0).flex().child(button("Save").on_click(click)));
    ui.pointer_moved(Point::new(20.0, 20.0));
    assert_eq!(ui.cursor(), Cursor::Pointer);
    ui.click_at(20.0, 20.0);
    assert_eq!(clicks.get(), 1);
    assert!(ui.press(Key::Enter));
    assert!(ui.press(Key::Space));
    assert!(!ui.press(Key::Escape), "other keys are left alone");
    assert_eq!(clicks.get(), 3);
}

#[test]
fn a_disabled_button_ignores_input_and_cannot_be_focused() {
    let _entered = Runtime::new().enter();
    let enabled = signal(false);
    let (clicks, click) = counter();
    let mut ui = builder().build(move || {
        div()
            .p(10.0)
            .flex()
            .child(button("Save").enabled(enabled).on_click(click))
    });
    ui.click_at(20.0, 20.0);
    ui.press(Key::Tab);
    assert!(!ui.has_focus());
    ui.press(Key::Enter);
    assert_eq!(clicks.get(), 0);
    enabled.set(true);
    ui.advance(0.0);
    ui.press(Key::Tab);
    ui.press(Key::Enter);
    assert_eq!(clicks.get(), 1);
}

#[test]
fn buttons_take_theme_colours_and_style_overrides() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| {
        div()
            .p(10.0)
            .flex()
            .gap(10.0)
            .child(button("Plain").w(80.0).h(30.0))
            .child(button("Go").primary().w(80.0).h(30.0))
    });
    let img = ui.render()?;
    let palette = Palette::light();
    assert_near(
        img.pixel(14, 14),
        palette.surface.to_rgba8(),
        "secondary uses the surface colour",
    );
    assert_near(
        img.pixel(104, 14),
        palette.accent.to_rgba8(),
        "primary uses the accent colour",
    );
    assert_eq!(
        ui.bounds(104.0, 14.0)?.size.width,
        80.0,
        "style methods override the defaults"
    );
    Ok(())
}
