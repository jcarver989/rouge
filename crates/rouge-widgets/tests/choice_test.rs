mod common;

use common::builder;
use rouge_core::testing::{TestResult, assert_near};
use rouge_core::view;
use rouge_core::{Color, PointerButton, Styled, UiPalette, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{radio, toggle};

#[test]
fn toggle_flips_its_signal_by_pointer_and_keyboard() -> TestResult {
    let _entered = Runtime::new().enter();
    let on = signal(false);
    let mut ui = builder().build(move || div().p(10.0).child(toggle(on)));
    let palette = UiPalette::light().map(Color::from);
    assert_near(
        ui.render()?.pixel(40, 20),
        palette.surface_selected.over(Color::WHITE).to_rgba8(),
        "unchecked, the track is the selected tint",
    );
    ui.click_at(20.0, 20.0);
    assert!(on.get());
    ui.press("space");
    assert!(!on.get(), "space toggles the focused switch");
    on.set(true);
    ui.run(0.6);
    assert_near(
        ui.render()?.pixel(14, 20),
        palette.accent.to_rgba8(),
        "the track follows the signal",
    );
    Ok(())
}

#[test]
fn the_toggle_thumb_stretches_while_pressed_and_flips_on_release() -> TestResult {
    let _entered = Runtime::new().enter();
    let on = signal(false);
    let mut ui = builder().build(move || {
        view! {
            <div p=10.0>
                <toggle checked=on test_id="wifi"/>
            </div>
        }
    });
    let rest = ui.rect_of("wifi-thumb")?.size.width;
    let center = ui.center_of("wifi")?;
    ui.pointer_moved(center);
    ui.pointer_down(PointerButton::Primary);
    ui.run(0.5);
    assert_eq!(ui.rect_of("wifi-thumb")?.size.width, rest + 4.0);
    assert!(!on.get(), "pressing alone does not flip the switch");
    ui.pointer_up(PointerButton::Primary);
    ui.run(0.5);
    assert_eq!(ui.rect_of("wifi-thumb")?.size.width, rest);
    assert!(on.get());
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
    ui.press("tab");
    ui.press("tab");
    ui.press("space");
    assert_eq!(size.get(), "l");
}

#[test]
fn a_radio_dot_grows_in_without_remounting_and_then_idles() -> TestResult {
    let _entered = Runtime::new().enter();
    let size = signal("m");
    let mut ui = builder().build(move || {
        view! {
            <div p=10.0 flex col items_start gap=10.0>
                <radio selected=size value="s" label="Small"/>
                <radio selected=size value="m" label="Medium"/>
            </div>
        }
    });
    ui.run(0.5);
    let created = ui.stats().elements_created;
    let accent = UiPalette::light().map(Color::from).accent.to_rgba8();
    assert_ne!(ui.pixel(14, 20)?, accent, "an unchosen radio is not filled");
    size.set("s");
    ui.advance(0.0);
    assert!(ui.is_animating(), "the dot animates in");
    ui.run(1.0);
    assert_near(
        ui.pixel(14, 20)?,
        accent,
        "the chosen radio is an accent fill",
    );
    assert_eq!(ui.pixel(19, 19)?, [255; 4], "with a white dot");
    assert_eq!(ui.stats().elements_created, created, "no remount");
    assert_eq!(ui.compositor_frames(1.0)?, 0);
    Ok(())
}
