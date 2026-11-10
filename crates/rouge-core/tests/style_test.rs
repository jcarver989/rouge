use rouge_core::testing::{BLUE, GREEN, TestResult, TestUi};
use rouge_core::{Color, Rect, Styled, div};
use rouge_reactive::Runtime;

#[test]
fn borders_take_layout_space_and_paint() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div()
            .size_full()
            .border(10.0)
            .border_color(Color::BLUE)
            .bg(Color::RED)
            .child(div().size_full().bg(Color::GREEN))
    });
    let img = ui.render()?;
    assert_eq!(img.pixel(5, 50), BLUE, "border");
    assert_eq!(img.pixel(15, 50), GREEN, "child starts inside the border");
    assert_eq!(ui.bounds(100.0, 50.0)?, Rect::new(10.0, 10.0, 180.0, 80.0));
    Ok(())
}

#[test]
fn flex_row_lays_children_out_with_gap_and_padding() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div()
            .flex()
            .size_full()
            .p(10.0)
            .gap(20.0)
            .child(div().w(50.0).h(30.0))
            .child(div().flex_1().h_full())
    });
    assert_eq!(ui.bounds(11.0, 11.0)?, Rect::new(10.0, 10.0, 50.0, 30.0));
    assert_eq!(
        ui.bounds(150.0, 50.0)?,
        Rect::new(80.0, 10.0, 110.0, 80.0),
        "takes the rest"
    );
    Ok(())
}

#[test]
fn block_children_stack_and_column_centering_works() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        let centered = div().flex().col().items_center().justify_center().h(80.0);
        div()
            .size_full()
            .child(div().h(20.0))
            .child(centered.child(div().size(40.0)))
    });
    assert_eq!(ui.bounds(5.0, 5.0)?, Rect::new(0.0, 0.0, 200.0, 20.0));
    assert_eq!(ui.bounds(100.0, 60.0)?, Rect::new(80.0, 40.0, 40.0, 40.0));
    Ok(())
}

#[test]
fn absolute_positioning_uses_insets() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div()
            .size_full()
            .child(div().absolute().right(10.0).bottom(5.0).w(30.0).h(20.0))
    });
    assert_eq!(ui.bounds(175.0, 85.0)?, Rect::new(160.0, 75.0, 30.0, 20.0));
    Ok(())
}
