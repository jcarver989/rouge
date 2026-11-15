use rouge_core::testing::TestResult;
use rouge_core::{Styled, div, view};
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

#[test]
fn losing_focus_commits_the_edit_once() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, commits) = component_test().size(240.0, 120.0).build_with(|| {
        let value = signal("draft".to_owned());
        let commits = signal(Vec::new());
        let commit = move |text: String| commits.update(|all| all.push(text));
        (
            view! {
                <div flex col size_full>
                    <inline_edit value test_id="title" on_commit=commit/>
                    <div flex_1 test_id="elsewhere"/>
                </div>
            },
            commits,
        )
    });
    ui.type_text("final");
    ui.click_on("elsewhere")?;
    assert_eq!(commits.get(), vec!["final".to_owned()]);
    Ok(())
}

#[test]
fn a_cancelled_edit_does_not_commit_when_focus_leaves() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, (commits, cancelled)) = component_test().size(240.0, 120.0).build_with(|| {
        let value = signal("draft".to_owned());
        let (commits, cancelled) = (signal(0), signal(false));
        (
            view! {
                <div flex col size_full>
                    <inline_edit value on_commit={move |_: String| commits.update(|n| *n += 1)}
                                 on_cancel={move || cancelled.set(true)}/>
                    <div flex_1 test_id="elsewhere"/>
                </div>
            },
            (commits, cancelled),
        )
    });
    ui.press("escape");
    ui.click_on("elsewhere")?;
    assert!(cancelled.get());
    assert_eq!(commits.get(), 0);
    Ok(())
}
