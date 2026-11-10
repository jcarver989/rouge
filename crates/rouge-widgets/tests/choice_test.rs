mod common;

use common::builder;
use rouge_core::testing::{TestResult, assert_near};
use rouge_core::{Key, Palette, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{checkbox, radio, toggle};

#[test]
fn toggle_and_checkbox_flip_their_signal_by_pointer_and_keyboard() -> TestResult {
    let _entered = Runtime::new().enter();
    let (on, checked) = (signal(false), signal(false));
    let mut ui = builder().build(move || {
        div()
            .p(10.0)
            .flex()
            .col()
            .items_start()
            .gap(10.0)
            .child(toggle(on))
            .child(checkbox(checked).label("Remember me"))
    });
    ui.click_at(20.0, 20.0);
    assert!(on.get());
    ui.press(Key::Space);
    assert!(!on.get(), "space toggles the focused switch");
    ui.click_at(60.0, 48.0);
    assert!(checked.get(), "clicking the label toggles the checkbox");
    ui.run(0.6);
    let img = ui.render()?;
    assert_near(
        img.pixel(12, 46),
        Palette::light().accent.to_rgba8(),
        "a checked box is filled with the accent",
    );
    ui.press(Key::Enter);
    assert!(!checked.get());
    on.set(true);
    ui.run(0.6);
    assert_near(
        ui.render()?.pixel(14, 20),
        Palette::light().accent.to_rgba8(),
        "the track follows the signal",
    );
    Ok(())
}

#[test]
fn radios_share_one_signal() {
    let _entered = Runtime::new().enter();
    let size = signal("m");
    let mut ui = builder().build(move || {
        div()
            .p(10.0)
            .flex()
            .col()
            .items_start()
            .gap(10.0)
            .child(radio(size, "s").label("Small"))
            .child(radio(size, "m").label("Medium"))
            .child(radio(size, "l").label("Large"))
    });
    ui.click_at(18.0, 18.0);
    assert_eq!(size.get(), "s");
    ui.press(Key::Tab);
    ui.press(Key::Tab);
    ui.press(Key::Space);
    assert_eq!(size.get(), "l");
}
