use rouge_core::access::{Action, Role};
use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{Styled, div};
use rouge_reactive::{Runtime, keyed_list};
use rouge_widgets::tag_input;
use rouge_widgets::testing::component_test;

#[test]
fn each_tag_has_a_named_remove_button() -> TestResult {
    let _entered = Runtime::new().enter();
    let tags = keyed_list(String::clone);
    tags.push("ambient".to_owned());
    let mut ui = component_test()
        .size(320.0, 120.0)
        .build(move || div().child(tag_input(tags).access_label("Tags")));
    ui.access_node(Role::TextInput, "Tags")?;
    assert!(ui.perform(Role::Button, "Remove ambient", Action::Click)?);
    assert!(tags.keys().with_untracked(Vec::is_empty));
    Ok(())
}

#[test]
fn enter_adds_a_tag_backspace_removes_the_last_and_each_can_be_removed() -> TestResult {
    let _entered = Runtime::new().enter();
    let tags = keyed_list(String::clone);
    tags.push("ambient".to_owned());
    let mut ui = component_test().size(320.0, 120.0).build(move || {
        view! {
            <div p=10.0>
                <tag_input tags=tags test_id="tags" placeholder="Add a tag" w=280.0/>
            </div>
        }
    });
    let names = move || tags.keys().get();
    ui.run(0.5);
    assert!(ui.bounds_of("tags-tag-ambient").is_some());
    ui.click_on("tags-input")?;
    ui.type_text(" late night ");
    ui.press("enter");
    ui.type_text("ambient");
    ui.press("enter");
    assert_eq!(
        names(),
        ["ambient", "late night"],
        "trimmed, and never twice"
    );
    ui.advance(0.0);
    assert!(ui.is_animating(), "the new tag pops in");
    ui.run(0.5);
    let (first, second) = (
        ui.rect_of("tags-tag-ambient")?,
        ui.rect_of("tags-tag-late night")?,
    );
    assert!(
        first.max().x <= second.origin.x,
        "tags sit in order before the input"
    );
    assert!(second.max().x <= ui.rect_of("tags-input")?.origin.x);
    ui.click_on("tags-remove-ambient")?;
    assert_eq!(names(), ["late night"]);
    ui.type_text("x");
    ui.press("backspace");
    assert_eq!(names(), ["late night"], "backspace edits the draft first");
    ui.press("backspace");
    assert!(names().is_empty(), "then removes the last tag");
    ui.run(1.0);
    assert!(ui.bounds_of("tags-tag-late night").is_none());
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}
