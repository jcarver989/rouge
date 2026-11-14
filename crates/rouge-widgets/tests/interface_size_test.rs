use rouge_core::testing::{TestResult, TestUi};
use rouge_core::{Styled, div};
use rouge_reactive::Runtime;
use rouge_widgets::testing::component_test;
use rouge_widgets::*;

#[test]
fn interface_size_zooms_the_window_through_shortcuts_and_preview_transactions() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, size) = component_test().build_with(|| {
        let size = InterfaceSize::install(18.0);
        (
            size.bind(
                div()
                    .autofocus()
                    .size_full()
                    .child(div().test_id("ruler").w(32.0).h(16.0)),
            ),
            size,
        )
    });
    let zoom = |ui: &mut TestUi| {
        ui.advance(0.0);
        ui.scale() * tokens::BASE_SIZE
    };
    assert_eq!(zoom(&mut ui), 18.0);
    assert_eq!(ui.rect_of("ruler")?.size.width, 32.0);
    ui.press("secondary-=");
    assert_eq!(zoom(&mut ui), 19.0);
    size.preview(22.0);
    assert_eq!(size.committed(), 19.0);
    assert_eq!(zoom(&mut ui), 22.0);
    size.cancel();
    assert_eq!(zoom(&mut ui), 19.0);
    size.preview(100.0);
    size.commit();
    assert_eq!(zoom(&mut ui), 32.0);
    assert_eq!(ui.bounds(1.0, 1.0)?.size.width, 32.0);
    size.set(-100.0);
    assert_eq!(zoom(&mut ui), 10.0);
    size.apply(SizeAction::Reset);
    assert_eq!(zoom(&mut ui), 18.0);
    assert_eq!(ui.rect_of("ruler")?.size.width, 32.0);
    Ok(())
}
