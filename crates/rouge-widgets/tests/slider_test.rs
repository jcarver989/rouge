mod common;

use common::builder;
use rouge_core::access::{Action, Role};
use rouge_core::testing::{TestResult, TestUi};
use rouge_core::view;
use rouge_core::{Point, PointerButton, Rect, Styled, div};
use rouge_reactive::{Runtime, Signal, signal};
use rouge_widgets::slider;
use rouge_widgets::testing::component_test;

#[test]
fn a_slider_reports_its_value_and_steps_by_accessibility_actions() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, value) = stepped_slider(40.0);
    let node = ui.access_node(Role::Slider, "Volume")?;
    assert_eq!(node.numeric_value, Some(40.0));
    assert_eq!(node.value.as_deref(), Some("40%"));
    assert!(ui.perform(Role::Slider, "Volume", Action::Increment)?);
    assert_eq!(value.get(), 50.0);
    assert!(ui.perform(Role::Slider, "Volume", Action::Decrement)?);
    assert!(ui.perform(Role::Slider, "Volume", Action::Decrement)?);
    assert_eq!(value.get(), 30.0);
    assert_eq!(
        ui.access_node(Role::Slider, "Volume")?.value.as_deref(),
        Some("30%")
    );
    Ok(())
}

#[test]
fn slider_follows_the_pointer_snaps_to_steps_and_takes_arrow_keys() {
    let _entered = Runtime::new().enter();
    let (mut ui, value) = stepped_slider(0.0);
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
            .child(slider(value).range((0.0, 100.0)).w(216.0))
    });
    assert_eq!(ui.bounds(119.0, 18.0)?, Rect::new(111.0, 10.0, 16.0, 16.0));
    assert_eq!((ui.stats().layouts, ui.stats().relayouts), (1, 0));
    value.set(100.0);
    ui.advance(0.0);
    assert_eq!(ui.bounds(219.0, 18.0)?, Rect::new(211.0, 10.0, 16.0, 16.0));
    Ok(())
}

#[test]
fn a_formatted_slider_shows_a_value_tip_while_hovered_or_dragged() -> TestResult {
    let _entered = Runtime::new().enter();
    let value = signal(50.0f32);
    let mut ui = component_test().size(300.0, 120.0).build(move || {
        view! {
            <div pt=60.0 px=10.0>
                <slider value=value test_id="volume" range={(0.0, 100.0)} step=1.0
                     format={|value| format!("{value:.0}%")} w=216.0/>
            </div>
        }
    });
    assert!(ui.bounds_of("volume-tip").is_none());
    assert_eq!(
        ui.compositor_frames(1.0)?,
        0,
        "an idle slider draws nothing"
    );
    let center = ui.center_of("volume")?;
    ui.pointer_moved(center);
    ui.run(0.5);
    let tip = ui.rect_of("volume-tip")?;
    assert!(
        tip.max().y <= ui.rect_of("volume")?.origin.y,
        "above the thumb"
    );
    assert!(
        (tip.center().x - center.x).abs() <= 2.0,
        "centred on the thumb: {tip:?} over {center:?}"
    );
    assert!(ui.text_bounds("50%").is_some());
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_moved(center + Point::new(50.0, 80.0));
    ui.run(0.5);
    assert!(ui.text_bounds("75%").is_some(), "the tip follows the value");
    assert!(
        ui.bounds_of("volume-tip").is_some(),
        "it stays while dragging outside"
    );
    ui.pointer_up(PointerButton::Primary);
    ui.run(0.5);
    assert!(ui.bounds_of("volume-tip").is_none());
    assert_eq!(ui.compositor_frames(1.0)?, 0);
    Ok(())
}

fn stepped_slider(initial: f32) -> (TestUi, Signal<f32>) {
    let value = signal(initial);
    let ui = builder().build(move || {
        view! {
            <div p=10.0>
                <slider value=value range={(0.0, 100.0)} step=10.0 w=216.0 access_label="Volume"
                     format={|v| format!("{v:.0}%")}/>
            </div>
        }
    });
    (ui, value)
}
