mod common;

use common::builder;
use rouge_core::testing::TestResult;
use rouge_core::{
    Point, PointerButton, Rect, ResizeEdge, Styled, WindowCommand, div, window_handle,
};
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
    for (edge, bounds) in [
        (ResizeEdge::North, Rect::new(4.0, 0.0, 292.0, 4.0)),
        (ResizeEdge::South, Rect::new(4.0, 196.0, 292.0, 4.0)),
        (ResizeEdge::West, Rect::new(0.0, 4.0, 4.0, 192.0)),
        (ResizeEdge::East, Rect::new(296.0, 4.0, 4.0, 192.0)),
        (ResizeEdge::NorthWest, Rect::new(0.0, 0.0, 4.0, 4.0)),
        (ResizeEdge::NorthEast, Rect::new(296.0, 0.0, 4.0, 4.0)),
        (ResizeEdge::SouthWest, Rect::new(0.0, 196.0, 4.0, 4.0)),
        (ResizeEdge::SouthEast, Rect::new(296.0, 196.0, 4.0, 4.0)),
    ] {
        let id = format!("window-resize-{edge:?}");
        assert_eq!(ui.bounds_of(&id).ok_or("missing resize zone")?, bounds);
        ui.pointer_moved(bounds.center());
        ui.pointer_down(PointerButton::Primary);
        assert!(
            matches!(&ui.take_window_commands()[..], [WindowCommand::StartResize(actual)] if *actual == edge)
        );
        ui.pointer_up(PointerButton::Primary);
    }
    Ok(())
}
