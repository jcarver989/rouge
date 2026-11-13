use rouge_core::testing::{TestResult, TestUi};
use rouge_core::{Styled, div};
use rouge_reactive::Runtime;
use rouge_widgets::testing::component_test;
use rouge_widgets::*;

#[test]
fn mounted_rem_layout_follows_shortcuts_and_preview_transactions() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, size) = component_test().build_with(|| {
        let size = InterfaceSize::install(18.0);
        (
            size.bind(
                div()
                    .autofocus()
                    .child(div().test_id("ruler").w(32.0).h(16.0)),
            ),
            size,
        )
    });
    let width = |ui: &mut TestUi| ui.rect_of("ruler").map(|ruler| ruler.size.width);
    assert!((width(&mut ui)? - 36.0).abs() < 0.1);
    ui.press("secondary-=");
    assert!((width(&mut ui)? - 38.0).abs() < 0.1);
    size.preview(22.0);
    assert!((size.committed() - 19.0).abs() < 0.1);
    assert!((width(&mut ui)? - 44.0).abs() < 0.1);
    size.cancel();
    assert!((width(&mut ui)? - 38.0).abs() < 0.1);
    size.preview(100.0);
    size.commit();
    assert!((width(&mut ui)? - 64.0).abs() < 0.1);
    size.set(-100.0);
    assert!((width(&mut ui)? - 20.0).abs() < 0.1);
    size.apply(SizeAction::Reset);
    assert!((width(&mut ui)? - 36.0).abs() < 0.1);
    Ok(())
}
