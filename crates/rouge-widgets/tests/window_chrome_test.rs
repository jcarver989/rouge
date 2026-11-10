mod common;

use common::builder;
use rouge_core::testing::TestResult;
use rouge_core::{Point, PointerButton, ResizeEdge, Styled, WindowCommand, div, window_handle};
use rouge_reactive::Runtime;
use rouge_widgets::window_chrome;

#[test]
fn chrome_reserves_content_space_and_routes_controls_to_the_window() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = builder().build(|| window_chrome(window_handle().title(), div().size_full()));
    let content = ui.bounds_of("window-content").ok_or("content")?;
    assert_eq!(content.origin.y, 36.0);
    assert_eq!(content.size.height, 164.0);
    ui.click_on("window-minimize")?;
    assert!(matches!(
        &ui.take_window_commands()[..],
        [WindowCommand::Minimize]
    ));
    ui.click_on("window-zoom")?;
    assert!(matches!(
        &ui.take_window_commands()[..],
        [WindowCommand::Zoom]
    ));
    ui.click_on("window-close")?;
    assert!(matches!(
        &ui.take_window_commands()[..],
        [WindowCommand::Close]
    ));
    ui.pointer_moved(Point::new(20.0, 18.0));
    ui.pointer_down(PointerButton::Primary);
    assert!(matches!(
        &ui.take_window_commands()[..],
        [WindowCommand::StartDrag]
    ));
    ui.pointer_up(PointerButton::Primary);
    ui.pointer_moved(Point::new(1.0, 1.0));
    ui.pointer_down(PointerButton::Primary);
    assert!(matches!(
        &ui.take_window_commands()[..],
        [WindowCommand::StartResize(ResizeEdge::NorthWest)]
    ));
    Ok(())
}
