use rouge_core::testing::{TestResult, assert_near};
use rouge_core::view;
use rouge_core::{Color, Styled, UiPalette, div};
use rouge_reactive::Runtime;
use rouge_widgets::skeleton;
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::MotionScale;

#[test]
fn a_skeleton_is_a_placeholder_block_sized_by_its_caller_and_still_without_motion() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test()
        .size(200.0, 80.0)
        .clear(Color::WHITE)
        .build(|| {
            MotionScale::None.provide();
            view! {
                <div p=10.0 flex col gap=8.0>
                    <div test_id="line">
                        <skeleton/>
                    </div>
                    <div test_id="block">
                        <skeleton w=40.0 h=24.0/>
                    </div>
                </div>
            }
        });
    assert_eq!(ui.rect_of("line")?.size.height, 16.0);
    assert_eq!(ui.rect_of("line")?.size.width, 180.0, "it fills its row");
    let block = ui.rect_of("block")?;
    assert_eq!(block.size.height, 24.0);
    let fill = UiPalette::light().map(Color::from).surface_selected;
    assert_near(
        ui.pixel(block.origin.x as u32 + 20, block.center().y as u32)?,
        fill.over(Color::WHITE).to_rgba8(),
        "a quiet fill",
    );
    assert_eq!(
        ui.pixel(block.origin.x as u32 + 60, block.center().y as u32)?,
        [255; 4]
    );
    assert_eq!(
        ui.compositor_frames(1.0)?,
        0,
        "it shimmers only when motion is on"
    );
    Ok(())
}

#[test]
fn a_skeleton_shimmers_while_motion_is_on() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test()
        .size(300.0, 60.0)
        .clear(Color::WHITE)
        .build(|| {
            view! {
                <div p=10.0>
                    <skeleton w=240.0 h=20.0/>
                </div>
            }
        });
    let mut seen = Vec::new();
    for _ in 0..8 {
        ui.advance(0.2);
        seen.push(ui.pixel(130, 20)?);
    }
    seen.dedup();
    assert!(seen.len() > 1, "{seen:?}");
    assert!(ui.compositor_frames(1.0)? > 0);
    Ok(())
}
