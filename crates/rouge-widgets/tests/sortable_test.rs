use rouge_core::testing::{TestResult, TestUi};
use rouge_core::view;
use rouge_core::{Point, PointerButton, Styled, div, text};
use rouge_reactive::{KeyedList, Runtime, keyed_list};
use rouge_widgets::sortable;
use rouge_widgets::testing::component_test;

#[test]
fn dragging_a_row_past_its_neighbours_reorders_the_list_on_release() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, queue) = queue();
    let order = move || queue.keys().get();
    let (first, third) = (ui.center_of("one")?, ui.rect_of("three")?);
    ui.pointer_moved(first);
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_moved(first + Point::new(0.0, 30.0));
    ui.pointer_moved(first + Point::new(0.0, 62.0));
    ui.run(0.5);
    assert_eq!(
        order(),
        ["one", "two", "three"],
        "nothing is written mid-drag"
    );
    assert_eq!(
        ui.rect_of("one")?.center().y,
        first.y + 62.0,
        "the dragged row follows the pointer"
    );
    assert!(
        (ui.rect_of("three")?.origin.y - (third.origin.y - 30.0)).abs() < 0.5,
        "the rows it passed make room"
    );
    ui.pointer_up(PointerButton::Primary);
    assert_eq!(order(), ["two", "three", "one"]);
    ui.advance(0.0);
    assert!(ui.is_animating(), "the row settles into its slot");
    ui.run(1.5);
    assert!((ui.rect_of("one")?.origin.y - third.origin.y).abs() < 0.5);
    assert!((ui.rect_of("two")?.center().y - first.y).abs() < 0.5);
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

#[test]
fn a_focused_row_moves_with_alt_and_the_arrow_keys() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, queue) = queue();
    let order = move || queue.keys().get();
    ui.press("tab");
    ui.press("down");
    assert_eq!(order(), ["one", "two", "three"], "plain arrows do nothing");
    ui.press("alt-down");
    assert_eq!(order(), ["two", "one", "three"]);
    ui.press("alt-down");
    ui.press("alt-down");
    assert_eq!(order(), ["two", "three", "one"], "it stops at the end");
    ui.press("alt-up");
    assert_eq!(order(), ["two", "one", "three"]);
    ui.run(1.0);
    assert_eq!(ui.compositor_frames(1.0)?, 0);
    Ok(())
}

fn queue() -> (TestUi, KeyedList<&'static str, &'static str>) {
    let queue = keyed_list(|name: &&'static str| *name);
    queue.extend(["one", "two", "three"]);
    let ui = component_test().size(240.0, 200.0).build(move || {
        let rows = view! {
            <sortable list=queue
                 row={|name, _| {
                     view! {
                         <div test_id={*name} h=28.0 w=200.0>
                             <text>{*name}</text>
                         </div>
                     }
                 }}/>
        };
        view! { <div p=10.0>{rows}</div> }
    });
    (ui, queue)
}
