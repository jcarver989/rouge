use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{InputKind, Point, Styled, TouchPhase, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::number_field;
use rouge_widgets::testing::component_test;
use std::thread::sleep;
use std::time::Duration;

#[test]
fn a_number_is_typed_stepped_with_arrows_and_scrubbed_by_its_label() -> TestResult {
    let _entered = Runtime::new().enter();
    let gain = signal(2.0f64);
    let mut ui = component_test().size(300.0, 80.0).build(move || {
        view! {
            <div p=10.0 flex>
                <number_field value=gain test_id="gain" label="Gain" range={(-12.0, 12.0)} step=0.5 suffix="dB"/>
            </div>
        }
    });
    assert!(
        ui.text_bounds("2.0").is_some(),
        "shown at the step's precision"
    );
    assert!(ui.text_bounds("dB").is_some());
    ui.click_on("gain-input")?;
    ui.press("up");
    ui.press("up");
    ui.press("down");
    assert_eq!(gain.get(), 2.5);
    ui.press("secondary-a");
    ui.type_text("40");
    assert_eq!(
        gain.get(),
        2.5,
        "typing does not write until it is committed"
    );
    ui.press("enter");
    assert_eq!(gain.get(), 12.0, "a typed value is clamped to the range");
    ui.run(0.1);
    assert!(ui.text_bounds("12.0").is_some());
    ui.press("secondary-a");
    ui.type_text("loud");
    ui.press("enter");
    ui.run(0.1);
    assert_eq!(gain.get(), 12.0, "text that is not a number is discarded");
    assert!(ui.text_bounds("12.0").is_some());
    let label = ui.center_of("gain-label")?;
    ui.drag(label, label + Point::new(-16.0, 30.0));
    assert_eq!(gain.get(), 10.0, "four pixels of drag is one step");
    ui.drag(label, label + Point::new(-400.0, 0.0));
    assert_eq!(gain.get(), -12.0, "scrubbing stops at the range");
    assert_eq!(ui.compositor_frames(1.0)?, 0);
    Ok(())
}

#[test]
fn on_touch_a_number_is_stepped_by_buttons_that_repeat_while_held() -> TestResult {
    let _entered = Runtime::new().enter();
    let guests = signal(2.0f64);
    let mut ui = component_test()
        .size(320.0, 80.0)
        .input(InputKind::Touch)
        .build(move || {
            view! {
                <div p=8.0 flex>
                    <number_field value=guests test_id="guests" label="Guests" range={(1.0, 12.0)}/>
                </div>
            }
        });
    let (field, plus) = (ui.rect_of("guests")?, ui.rect_of("guests-increment")?);
    assert_eq!(field.size.height, 44.0);
    assert_eq!(
        (plus.size.width, plus.max().x),
        (44.0, field.max().x - 1.0),
        "the buttons are touch sized and sit at the trailing edge"
    );
    ui.tap_on("guests-increment")?;
    assert_eq!(guests.get(), 3.0);
    ui.tap_on("guests-decrement")?;
    ui.tap_on("guests-decrement")?;
    ui.tap_on("guests-decrement")?;
    assert_eq!(guests.get(), 1.0, "the range still clamps");
    let label = ui.center_of("guests-label")?;
    ui.swipe(label, Point::new(label.x + 40.0, label.y));
    assert_eq!(guests.get(), 1.0, "the label does not scrub on touch");
    ui.touch(TouchPhase::Start, plus.center());
    assert!(
        ui.wait_until(|_| guests.get() >= 4.0),
        "holding a button repeats it"
    );
    ui.touch(TouchPhase::End, plus.center());
    let held = guests.get();
    sleep(Duration::from_millis(200));
    ui.advance(0.0);
    assert_eq!(
        guests.get(),
        held,
        "releasing stops the repeat and adds no extra step"
    );
    Ok(())
}
