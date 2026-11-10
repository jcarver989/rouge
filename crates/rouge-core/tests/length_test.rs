use rouge_core::testing::{TestResult, TestUi};
use rouge_core::{Length, Rect, Styled, auto, div, pct, px};
use rouge_reactive::Runtime;

#[test]
fn lengths_resolve_as_pixels_fractions_of_the_parent_or_automatically() -> TestResult {
    let _rt = Runtime::new().enter();
    assert_eq!(
        (px(8.0), pct(25.0), auto()),
        (Length::Px(8.0), Length::Fraction(0.25), Length::Auto)
    );
    let mut ui = TestUi::new(|| {
        div()
            .size_full()
            .child(div().w(px(40.0)).h(10.0))
            .child(div().w(pct(25.0)).h(10.0))
            .child(div().w(auto()).h(10.0))
    });
    assert_eq!(ui.bounds(1.0, 5.0)?, Rect::new(0.0, 0.0, 40.0, 10.0));
    assert_eq!(ui.bounds(1.0, 15.0)?, Rect::new(0.0, 10.0, 50.0, 10.0));
    assert_eq!(ui.bounds(1.0, 25.0)?, Rect::new(0.0, 20.0, 200.0, 10.0));
    Ok(())
}
