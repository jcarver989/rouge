mod common;

use common::handheld_test;
use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{Edges, Point, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{IconName, fab, row_action, tabs};

#[test]
fn the_button_sits_in_the_bottom_trailing_corner_above_the_safe_area() -> TestResult {
    let _entered = Runtime::new().enter();
    let composed = signal(0);
    let mut ui = handheld_test().build(move || {
        view! {
            <div relative size_full>
                <fab icon=IconName::Pencil label="Compose" test_id="compose"
                     on_click={move || composed.update(|count| *count += 1)}/>
            </div>
        }
    });
    let at = ui.rect_of("compose")?;
    assert_eq!((at.size.width, at.size.height), (56.0, 56.0));
    assert_eq!((at.max().x, at.max().y), (390.0 - 16.0, 700.0 - 16.0));
    ui.set_safe_area(Edges {
        bottom: 34.0,
        ..Edges::ZERO
    });
    ui.run(0.5);
    assert_eq!(ui.rect_of("compose")?.max().y, 700.0 - 34.0 - 16.0);
    ui.tap_on("compose")?;
    assert_eq!(composed.get(), 1);
    ui.press("tab");
    ui.press("enter");
    assert_eq!(composed.get(), 2, "it is keyboard operable");
    Ok(())
}

#[test]
fn inside_a_tab_bar_the_button_clears_the_bar() -> TestResult {
    let _entered = Runtime::new().enter();
    let tab = signal(0usize);
    let mut ui = handheld_test().build(move || {
        let inbox = || {
            view! {
                <div relative size_full>
                    <fab icon=IconName::Pencil label="Compose" test_id="compose"/>
                </div>
            }
        };
        view! { <tabs selected=tab test_id="tabs" bar tab=("Inbox", inbox) tab=("You", || view! { <div/> }) size_full/> }
    });
    ui.run(1.0);
    let (bar, button) = (ui.rect_of("tabs-bar")?, ui.rect_of("compose")?);
    assert_eq!(
        button.max().y,
        bar.origin.y - 16.0,
        "{button:?} above {bar:?}"
    );
    ui.set_keyboard_inset(320.0);
    ui.run(0.5);
    assert_eq!(
        ui.rect_of("compose")?.max().y,
        700.0 - 320.0 - 16.0,
        "and the keyboard once the bar has gone"
    );
    Ok(())
}

#[test]
fn with_actions_a_press_opens_a_speed_dial_over_a_scrim() -> TestResult {
    let _entered = Runtime::new().enter();
    let (picked, pressed) = (signal(""), signal(0));
    let mut ui = handheld_test().build(move || {
        view! {
            <div relative size_full>
                <fab icon=IconName::Plus label="New" test_id="new"
                     on_click={move || pressed.update(|count| *count += 1)}
                     action={row_action("Voice memo", move || picked.set("memo")).icon(IconName::AudioLines).test_id("memo")}
                     action={row_action("New message", move || picked.set("message")).icon(IconName::Pencil).test_id("message")}/>
            </div>
        }
    });
    assert_eq!(ui.bounds_of("new-dial"), None);
    ui.tap_on("new")?;
    ui.run(0.5);
    assert_eq!(pressed.get(), 0, "the press opens the dial instead");
    let (button, close, memo, message) = (
        ui.rect_of("new")?,
        ui.rect_of("new-close")?,
        ui.rect_of("memo")?,
        ui.rect_of("message")?,
    );
    assert_eq!(close, button, "a close button covers the dial's own button");
    assert!(
        memo.max().y <= message.origin.y && message.max().y <= close.origin.y,
        "the actions rise from it in order"
    );
    assert!(ui.text_bounds("Voice memo").is_some());
    ui.tap_on("message")?;
    ui.run(0.5);
    assert_eq!(picked.get(), "message");
    assert_eq!(ui.bounds_of("new-dial"), None, "choosing closes the dial");
    ui.tap_on("new")?;
    ui.run(0.5);
    ui.tap(Point::new(40.0, 40.0));
    ui.run(0.5);
    assert_eq!(ui.bounds_of("new-dial"), None, "so does a press outside");
    assert_eq!(picked.get(), "message");
    Ok(())
}
