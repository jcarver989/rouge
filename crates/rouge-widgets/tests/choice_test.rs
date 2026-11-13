mod common;

use common::builder;
use rouge_core::testing::{TestResult, assert_near};
use rouge_core::{Color, Styled, UiPalette, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{radio, toggle};

#[test]
fn toggle_flips_its_signal_by_pointer_and_keyboard() -> TestResult {
    let _entered = Runtime::new().enter();
    let on = signal(false);
    let mut ui = builder().build(move || div().p(10.0).child(toggle(on)));
    ui.click_at(20.0, 20.0);
    assert!(on.get());
    ui.press("space");
    assert!(!on.get(), "space toggles the focused switch");
    on.set(true);
    ui.run(0.6);
    assert_near(
        ui.render()?.pixel(14, 20),
        UiPalette::light().map(Color::from).accent.to_rgba8(),
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
            .child(radio(size, "s", Some("Small".to_owned())))
            .child(radio(size, "m", Some("Medium".to_owned())))
            .child(radio(size, "l", Some("Large".to_owned())))
    });
    ui.click_at(18.0, 18.0);
    assert_eq!(size.get(), "s");
    ui.press("tab");
    ui.press("tab");
    ui.press("space");
    assert_eq!(size.get(), "l");
}
