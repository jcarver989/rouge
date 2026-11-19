use rouge_core::testing::TestResult;
use rouge_core::{Styled, div, view};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::*;

#[test]
fn shows_the_value_until_double_clicked() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().build(|| {
        let value = signal("original".to_owned());
        view! { <inline_edit value test_id="title"/> }
    });
    assert_eq!(ui.text_of("title").as_deref(), Some("original"));
    ui.type_text("ignored");
    ui.click_on("title")?;
    ui.type_text("ignored");
    assert_eq!(ui.text_of("title").as_deref(), Some("original"));
    Ok(())
}

#[test]
fn double_click_selects_the_value_and_enter_reports_the_replacement() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, (value, submitted)) = component_test().build_with(|| {
        let value = signal("original".to_owned());
        let submitted = signal(String::new());
        (
            view! { <inline_edit value test_id="title" on_commit={move |text: String| submitted.set(text)}/> },
            (value, submitted),
        )
    });
    ui.double_click_on("title")?;
    ui.type_text("replacement");
    ui.press("enter");
    assert_eq!(submitted.get(), "replacement");
    assert_eq!(value.get(), "original");
    assert_eq!(ui.text_of("title").as_deref(), Some("original"));
    Ok(())
}

#[test]
fn an_edit_begun_by_its_owner_takes_typing_at_once_and_reports_the_replacement() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, submitted) = component_test().build_with(|| {
        let value = signal("original".to_owned());
        let submitted = signal(String::new());
        (
            view! { <inline_edit value test_id="title" begin_editing on_commit={move |text: String| submitted.set(text)}/> },
            submitted,
        )
    });
    ui.type_text("replacement");
    ui.press("enter");
    assert_eq!(submitted.get(), "replacement");
    assert_eq!(ui.text_of("title").as_deref(), Some("original"));
    Ok(())
}

#[test]
fn shows_the_bound_value_once_the_commit_writes_it() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().build(|| {
        let value = signal("original".to_owned());
        view! { <inline_edit value test_id="title" on_commit={move |text: String| value.set(text.trim().to_owned())}/> }
    });
    ui.double_click_on("title")?;
    ui.type_text("  replacement  ");
    ui.press("enter");
    assert_eq!(ui.text_of("title").as_deref(), Some("replacement"));
    Ok(())
}

#[test]
fn escape_cancels_and_keeps_the_value() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, (value, cancelled)) = component_test().build_with(|| {
        let cancelled = signal(false);
        let value = signal("value".to_owned());
        (
            view! { <inline_edit value test_id="title" on_cancel={move || cancelled.set(true)}/> },
            (value, cancelled),
        )
    });
    ui.double_click_on("title")?;
    ui.type_text("discarded");
    ui.press("escape");
    assert!(cancelled.get());
    assert_eq!(value.get(), "value");
    assert_eq!(ui.text_of("title").as_deref(), Some("value"));
    Ok(())
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
    ui.double_click_on("title")?;
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
                    <inline_edit value test_id="title" on_commit={move |_: String| commits.update(|n| *n += 1)}
                                 on_cancel={move || cancelled.set(true)}/>
                    <div flex_1 test_id="elsewhere"/>
                </div>
            },
            (commits, cancelled),
        )
    });
    ui.double_click_on("title")?;
    ui.press("escape");
    ui.click_on("elsewhere")?;
    assert!(cancelled.get());
    assert_eq!(commits.get(), 0);
    Ok(())
}

#[test]
fn editing_styles_apply_only_while_editing() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(240.0, 120.0).build(|| {
        let value = signal("draft".to_owned());
        view! {
            <div flex col size_full>
                <inline_edit value test_id="title" h=20.0 editing:h=40.0/>
            </div>
        }
    });
    assert_eq!(ui.rect_of("title")?.size.height, 20.0);
    ui.double_click_on("title")?;
    assert_eq!(ui.rect_of("title")?.size.height, 40.0);
    ui.press("escape");
    assert_eq!(ui.rect_of("title")?.size.height, 20.0);
    Ok(())
}
