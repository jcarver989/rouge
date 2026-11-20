use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{InputKind, Point, ScrollState, Scrollable, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{chip, chip_row};

#[test]
fn a_chip_toggles_its_filter_counts_and_offers_a_touch_target() -> TestResult {
    let _entered = Runtime::new().enter();
    let (unread, count) = (signal(false), signal(12usize));
    let mut ui = component_test()
        .size(390.0, 200.0)
        .input(InputKind::Touch)
        .build(move || {
            view! {
                <div flex p=16.0>
                    <chip label="Unread" test_id="unread" selected=unread count=count
                         on_click={move || unread.toggle()}/>
                </div>
            }
        });
    ui.run(0.5);
    let target = ui.rect_of("unread")?;
    assert_eq!(target.size.height, 44.0, "a 32 px chip in a 44 px target");
    assert!(ui.text_bounds("12").is_some());
    ui.tap(Point::new(target.center().x, target.origin.y + 2.0));
    assert!(unread.get(), "the whole target answers");
    ui.tap_on("unread")?;
    assert!(!unread.get());
    count.set(0);
    ui.run(0.5);
    assert_eq!(ui.text_bounds("0"), None, "zero draws no count");
    Ok(())
}

#[test]
fn a_row_of_chips_scrolls_sideways_when_it_overflows() -> TestResult {
    let _entered = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = component_test()
        .size(320.0, 200.0)
        .input(InputKind::Touch)
        .build(move || {
            let chips = [
                "All",
                "Unread",
                "Flagged",
                "Attachments",
                "Mentions",
                "Drafts",
            ]
            .map(|label| chip(label.to_owned()).test_id(label));
            div().flex().col().size_full().child(
                chip_row()
                    .children(chips)
                    .scroll_state(state)
                    .test_id("row"),
            )
        });
    ui.run(0.5);
    let (row, first, last) = (
        ui.rect_of("row")?,
        ui.rect_of("All")?,
        ui.rect_of("Drafts")?,
    );
    assert_eq!(row.size.height, 44.0);
    assert!(
        first.origin.x >= 0.0 && last.max().x > 320.0,
        "it overflows"
    );
    assert!(ui.rect_of("Unread")?.origin.x - first.max().x >= 8.0);
    ui.swipe_over(Point::new(280.0, 22.0), Point::new(60.0, 22.0), 0.2);
    ui.run(1.0);
    assert!(state.get().offset.x > 100.0, "a sideways swipe scrolls it");
    Ok(())
}
