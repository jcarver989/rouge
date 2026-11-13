use rouge_core::testing::{TestResult, counter};
use rouge_core::{Cursor, Styled, div, dynamic, view};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::button;
use rouge_widgets::testing::component_test;

#[test]
fn a_button_fires_on_click_and_on_enter_or_space_when_focused() -> TestResult {
    let _entered = Runtime::new().enter();
    let (clicks, click) = counter();
    let mut ui = component_test().build(move || {
        div()
            .p(10.0)
            .flex()
            .child(view! { <button id="save" label="Save" keyboard_focusable on_click=click/> })
    });
    let center = ui.center_of("save")?;
    ui.pointer_moved(center);
    assert_eq!(ui.cursor(), Cursor::Pointer);
    ui.click_on("save")?;
    assert_eq!(clicks.get(), 1);
    assert!(ui.press("enter"));
    assert!(ui.press("space"));
    assert!(!ui.press("escape"), "other keys are left alone");
    assert_eq!(clicks.get(), 3);
    Ok(())
}

#[test]
fn a_disabled_button_ignores_input_and_cannot_be_focused() -> TestResult {
    let _entered = Runtime::new().enter();
    let disabled = signal(true);
    let (clicks, click) = counter();
    let mut ui = component_test().build(move || {
        div().p(10.0).flex().child(dynamic(move || {
            let click = click.clone();
            view! { <button id="save" label="Save" disabled={disabled.get()} keyboard_focusable on_click=click/> }
        }))
    });
    ui.click_on("save")?;
    ui.press("tab");
    assert!(!ui.has_focus());
    ui.press("enter");
    assert_eq!(clicks.get(), 0);
    disabled.set(false);
    ui.advance(0.0);
    ui.press("tab");
    ui.press("enter");
    assert_eq!(clicks.get(), 1);
    Ok(())
}
