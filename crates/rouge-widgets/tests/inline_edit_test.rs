use rouge_core::view;
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::*;

#[test]
fn autofocus_selects_existing_value_and_enter_submits_replacement() {
    let _entered = Runtime::new().enter();
    let (mut ui, submitted) = component_test().build_with(|| {
        let value = signal("original".to_owned());
        let submitted = signal(String::new());
        (
            view! { <inline_edit value on_commit={move |text: String| submitted.set(text)}/> },
            submitted,
        )
    });
    ui.type_text("replacement");
    ui.press("enter");
    assert_eq!(submitted.get(), "replacement");
}

#[test]
fn escape_cancels() {
    let _entered = Runtime::new().enter();
    let (mut ui, cancelled) = component_test().build_with(|| {
        let cancelled = signal(false);
        let value = signal("value".to_owned());
        (
            view! { <inline_edit value on_cancel={move || cancelled.set(true)}/> },
            cancelled,
        )
    });
    ui.press("escape");
    assert!(cancelled.get());
}
