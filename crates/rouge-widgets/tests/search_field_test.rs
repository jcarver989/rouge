use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{InputKind, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::search_field;
use rouge_widgets::testing::component_test;

#[test]
fn on_touch_focus_brings_cancel_and_scopes_and_cancel_clears_and_leaves() -> TestResult {
    let _entered = Runtime::new().enter();
    let (query, scope, sent) = (signal(String::new()), signal(0usize), signal(String::new()));
    let mut ui = component_test()
        .size(390.0, 300.0)
        .input(InputKind::Touch)
        .build(move || {
            view! {
                <div flex col p=16.0 size_full>
                    <search_field value=query test_id="search" placeholder="Search"
                         scopes=(scope, ["All", "Unread", "Flagged"])
                         on_submit={move |text: String| sent.set(text)}/>
                </div>
            }
        });
    ui.run(0.5);
    let resting = ui.rect_of("search-frame")?;
    assert_eq!((resting.size.height, resting.size.width), (44.0, 358.0));
    assert_eq!(ui.bounds_of("search-cancel"), None);
    assert_eq!(ui.bounds_of("search-scopes"), None);
    assert_eq!(ui.bounds_of("search-clear"), None);
    ui.tap_on("search-input")?;
    ui.run(1.0);
    let (frame, cancel) = (ui.rect_of("search-frame")?, ui.rect_of("search-cancel")?);
    assert!(
        frame.size.width < resting.size.width && frame.max().x <= cancel.origin.x,
        "Cancel takes room beside the field"
    );
    assert_eq!(ui.rect_of("search-scopes")?.size.height, 44.0);
    ui.type_text("invo");
    ui.run(0.5);
    assert_eq!(query.get(), "invo");
    ui.press("enter");
    assert_eq!(sent.get(), "invo");
    ui.tap_on("search-clear")?;
    ui.run(0.5);
    assert_eq!(query.get(), "");
    assert!(
        ui.bounds_of("search-cancel").is_some(),
        "clearing keeps the search open"
    );
    ui.type_text("ab");
    ui.click_text("Unread")?;
    assert_eq!(scope.get(), 1);
    ui.tap_on("search-cancel")?;
    ui.run(1.0);
    assert_eq!(query.get(), "");
    assert_eq!(
        ui.bounds_of("search-cancel"),
        None,
        "Cancel ends the search"
    );
    assert_eq!(ui.bounds_of("search-scopes"), None);
    assert_eq!(ui.rect_of("search-frame")?.size.width, resting.size.width);
    Ok(())
}

#[test]
fn with_a_pointer_the_field_is_compact_and_has_no_cancel_button() -> TestResult {
    let _entered = Runtime::new().enter();
    let query = signal(String::new());
    let mut ui = component_test().size(390.0, 200.0).build(move || {
        view! {
            <div flex col p=16.0 size_full>
                <search_field value=query test_id="search"/>
            </div>
        }
    });
    assert_eq!(ui.rect_of("search-frame")?.size.height, 32.0);
    ui.click_on("search-input")?;
    ui.type_text("a");
    ui.run(0.5);
    assert_eq!(ui.bounds_of("search-cancel"), None);
    ui.click_on("search-clear")?;
    assert_eq!(query.get(), "");
    Ok(())
}
