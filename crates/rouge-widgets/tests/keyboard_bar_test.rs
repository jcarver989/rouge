mod common;

use common::handheld_test;
use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{ButtonVariant, action_bar, button, keyboard_bar, text_input};

#[test]
fn the_bar_rides_on_the_keyboard_steps_between_fields_and_done_takes_focus() -> TestResult {
    let _entered = Runtime::new().enter();
    let (name, steps, done) = (signal(String::new()), signal(0i32), signal(false));
    let mut ui = handheld_test().build(move || {
        view! {
            <div relative flex col p=16.0 size_full>
                <text_input value=name test_id="name"/>
                <keyboard_bar test_id="keys" on_next={move || steps.update(|at| *at += 1)}
                     on_previous={move || steps.update(|at| *at -= 1)}
                     on_done={move || done.set(true)}/>
                <action_bar test_id="footer">
                    <button label="Save".to_owned() variant=ButtonVariant::Primary/>
                </action_bar>
            </div>
        }
    });
    ui.run(0.5);
    assert_eq!(ui.bounds_of("keys"), None, "no keyboard, no bar");
    ui.tap_on("name")?;
    ui.set_keyboard_inset(300.0);
    ui.run(0.5);
    let bar = ui.rect_of("keys")?;
    assert_eq!(
        (bar.origin.x, bar.max().x, bar.max().y, bar.size.height),
        (12.0, 378.0, 700.0 - 300.0 - 8.0, 52.0),
        "a floating strip just above the keyboard"
    );
    assert!(
        ui.rect_of("footer")?.max().y <= bar.origin.y,
        "the page's own footer stacks above it"
    );
    ui.tap_on("keys-next")?;
    ui.tap_on("keys-next")?;
    ui.tap_on("keys-previous")?;
    assert_eq!(steps.get(), 1);
    ui.type_text("a");
    assert_eq!(name.get(), "", "stepping took focus from the field");
    ui.tap_on("name")?;
    ui.type_text("ok");
    assert_eq!(name.get(), "ok");
    ui.tap_on("keys-done")?;
    assert!(done.get());
    ui.type_text("x");
    assert_eq!(name.get(), "ok", "Done takes focus from the field");
    ui.set_keyboard_inset(0.0);
    ui.run(0.5);
    assert_eq!(ui.bounds_of("keys"), None);
    Ok(())
}
