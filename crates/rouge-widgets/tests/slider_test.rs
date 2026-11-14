mod common;

use common::builder;
use rouge_core::testing::TestResult;
use rouge_core::{Point, PointerButton, Rect, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::slider;

#[test]
fn slider_follows_the_pointer_snaps_to_steps_and_takes_arrow_keys() {
    let _entered = Runtime::new().enter();
    let value = signal(0.0f32);
    let mut ui = builder().build(move || {
        div()
            .p(10.0)
            .child(slider(value, Some((0.0, 100.0)), Some(10.0)).w(216.0))
    });
    ui.click_at(118.0, 18.0);
    assert_eq!(value.get(), 50.0, "the middle of the track");
    ui.pointer_moved(Point::new(118.0, 18.0));
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_moved(Point::new(176.0, 60.0));
    assert_eq!(
        value.get(),
        80.0,
        "dragging keeps tracking outside the slider and snaps to the step"
    );
    ui.pointer_moved(Point::new(900.0, 60.0));
    ui.pointer_up(PointerButton::Primary);
    assert_eq!(value.get(), 100.0, "clamped to the range");
    ui.pointer_moved(Point::new(30.0, 18.0));
    assert_eq!(value.get(), 100.0, "moves after release are ignored");
    ui.press("left");
    ui.press("left");
    assert_eq!(value.get(), 80.0);
    ui.press("home");
    assert_eq!(value.get(), 0.0);
    ui.press("end");
    assert_eq!(value.get(), 100.0);
}

#[test]
fn the_thumb_is_placed_by_layout_alone_on_the_first_frame() -> TestResult {
    let _entered = Runtime::new().enter();
    let value = signal(50.0f32);
    let mut ui = builder().build(move || {
        div()
            .p(10.0)
            .child(slider(value, Some((0.0, 100.0)), None).w(216.0))
    });
    assert_eq!(ui.bounds(119.0, 18.0)?, Rect::new(111.0, 10.0, 16.0, 16.0));
    assert_eq!((ui.stats().layouts, ui.stats().relayouts), (1, 0));
    value.set(100.0);
    ui.advance(0.0);
    assert_eq!(ui.bounds(219.0, 18.0)?, Rect::new(211.0, 10.0, 16.0, 16.0));
    Ok(())
}
