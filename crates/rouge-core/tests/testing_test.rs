use rouge_core::testing::{
    BLUE, LINEAR, RED, TestError, TestResult, TestUi, TestUiBuilder, WHITE, assert_near,
};
use rouge_core::{Color, Key, Point, Rect, Styled, div};
use rouge_reactive::{Runtime, signal};

#[test]
fn the_builder_sets_the_window_size_scale_and_clear_color() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default()
        .size(40.0, 20.0)
        .scale(2.0)
        .clear(Color::BLUE)
        .build(|| div().size(10.0).bg(Color::RED));
    let img = ui.render()?;
    assert_eq!((img.width, img.height), (80, 40));
    assert_eq!((img.pixel(10, 10), img.pixel(30, 30)), (RED, BLUE));
    assert_eq!(ui.first_x(BLUE, 5)?, 20);
    assert_near(TestUi::canvas(div).pixel(1, 1)?, WHITE, "a canvas is white");
    Ok(())
}

#[test]
fn build_with_hands_back_what_the_root_closure_made() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, clicks) = TestUiBuilder::default().build_with(|| {
        let clicks = signal(0);
        let target = div()
            .size(50.0)
            .focusable()
            .on_click(move |_| clicks.update(|count| *count += 1));
        (div().size_full().child(target), clicks)
    });
    assert_eq!(ui.bounds(10.0, 10.0)?, Rect::new(0.0, 0.0, 50.0, 50.0));
    ui.click(Point::new(10.0, 10.0));
    ui.click_at(20.0, 20.0);
    assert_eq!(clicks.get_untracked(), 2);
    assert!(!ui.press(Key::Enter), "nothing handles the key");
    assert!(matches!(
        TestUi::new(div).bounds(500.0, 500.0),
        Err(TestError::NoElement { .. })
    ));
    Ok(())
}

#[test]
fn run_and_wait_until_advance_the_clock_in_steps() -> TestResult {
    let _rt = Runtime::new().enter();
    let x = signal(0.0);
    let mut ui = TestUi::canvas(move || {
        let moving = div().size(20.0).bg(Color::RED).translate_x(x);
        div().size_full().child(moving.transition(LINEAR))
    });
    x.set(100.0);
    ui.run(0.5);
    assert_eq!(ui.first_x(RED, 10)?, 50);
    assert!(ui.wait_until(|ui| ui.first_x(RED, 10).is_ok_and(|x| x == 100)));
    assert!(ui.draw()?.layers > 0);
    ui.wait()?;
    Ok(())
}
