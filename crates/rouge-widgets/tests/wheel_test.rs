use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{InputKind, Point, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::wheel;

#[test]
fn a_wheel_holds_its_choice_in_the_middle_row_and_turns_by_drag_tap_and_arrows() -> TestResult {
    let _entered = Runtime::new().enter();
    let hour = signal(3usize);
    let mut ui = component_test()
        .size(200.0, 240.0)
        .input(InputKind::Touch)
        .build(move || {
            let hours = (0..12).map(|hour| format!("{hour:02}"));
            view! {
                <div flex p=20.0>
                    <wheel selected=hour labels=hours test_id="hour" w=80.0/>
                </div>
            }
        });
    ui.run(0.5);
    let (wheel, band) = (ui.rect_of("hour")?, ui.rect_of("hour-band")?);
    assert_eq!((wheel.size.height, band.size.height), (180.0, 36.0));
    assert_eq!(band.center().y, wheel.center().y);
    assert_eq!(
        ui.rect_of("hour-row-3")?,
        band,
        "the choice rests in the band"
    );
    ui.swipe_over(Point::new(60.0, 110.0), Point::new(60.0, 38.0), 2.0);
    ui.run(1.0);
    assert_eq!(hour.get(), 5, "a slow drag of two rows turns it two places");
    assert_eq!(ui.rect_of("hour-row-5")?, band, "and it settles on a row");
    ui.tap_on("hour-row-6")?;
    assert_eq!(hour.get(), 6, "a tap picks the row under it");
    ui.run(1.0);
    ui.swipe_over(Point::new(60.0, 60.0), Point::new(60.0, 200.0), 0.05);
    ui.run(1.5);
    assert_eq!(hour.get(), 0, "a fling stops at the first row");
    assert_eq!(ui.rect_of("hour-row-0")?, band);
    ui.press("tab");
    ui.press("down");
    ui.press("end");
    assert_eq!(hour.get(), 11);
    assert_eq!(ui.compositor_frames(2.0)?, 0, "nothing animates at rest");
    Ok(())
}
