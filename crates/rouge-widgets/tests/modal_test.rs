mod common;

use common::builder;
use rouge_core::testing::{TestResult, counter};
use rouge_core::{Key, Palette, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{button, modal};

#[test]
fn modal_covers_the_window_traps_focus_and_dismisses() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(false);
    let (behind, hit_behind) = counter();
    let (confirmed, confirm) = counter();
    let mut ui = builder().build(move || {
        let dialog = move || {
            let confirm = confirm.clone();
            div()
                .flex()
                .gap(8.0)
                .child(button("Cancel").on_click(move || open.set(false)))
                .child(button("OK").on_click(confirm))
        };
        div()
            .size_full()
            .child(button("Open").on_click(move || open.set(true)))
            .child(
                div()
                    .absolute()
                    .left(0.0)
                    .top(150.0)
                    .size(40.0)
                    .on_click(move |_| hit_behind()),
            )
            .child(modal(open, dialog))
    });
    ui.click_at(10.0, 10.0);
    ui.run(0.6);
    assert!(open.get());
    assert_ne!(
        ui.pixel(290, 190)?,
        Palette::light().surface.to_rgba8(),
        "the scrim dims the page"
    );
    ui.press(Key::Tab);
    ui.press(Key::Tab);
    ui.press(Key::Enter);
    assert_eq!(confirmed.get(), 1, "tab walks the dialog's buttons");
    ui.press(Key::Tab);
    ui.press(Key::Tab);
    ui.press(Key::Tab);
    ui.press(Key::Enter);
    assert_eq!(
        (open.get(), confirmed.get()),
        (true, 2),
        "tab wraps inside the dialog"
    );
    ui.press(Key::Escape);
    ui.run(0.6);
    assert!(!open.get(), "escape dismisses");
    ui.press(Key::Enter);
    ui.run(0.6);
    assert!(open.get(), "focus went back to the button that opened it");
    ui.click_at(20.0, 170.0);
    ui.run(0.6);
    assert_eq!(
        (open.get(), behind.get()),
        (false, 0),
        "a scrim click dismisses and never reaches the page"
    );
    Ok(())
}

#[test]
fn a_modal_that_is_not_dismissable_stays_open() {
    let _entered = Runtime::new().enter();
    let open = signal(true);
    let mut ui = builder().build(move || {
        div()
            .size_full()
            .child(modal(open, || div().size(50.0)).dismissable(false))
    });
    ui.run(0.6);
    ui.click_at(10.0, 10.0);
    ui.press(Key::Escape);
    assert!(open.get());
}
