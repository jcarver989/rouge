mod common;

use common::builder;
use rouge_core::testing::TestResult;
use rouge_core::{Point, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{Side, resizable_panel};

#[test]
fn panels_resize_with_correct_direction_clamp_and_reset_on_double_click() -> TestResult {
    for (side, end) in [
        (Side::Left, Point::new(250.0, 50.0)),
        (Side::Right, Point::new(-100.0, 50.0)),
        (Side::Top, Point::new(50.0, 250.0)),
        (Side::Bottom, Point::new(50.0, -100.0)),
    ] {
        let _rt = Runtime::new().enter();
        let size = signal(100.0);
        let seen = signal(0.0);
        let mut ui = builder().build(|| {
            div().size_full().child(
                resizable_panel(side, size)
                    .min_size(50.0)
                    .max_size(150.0)
                    .reset_size(90.0)
                    .on_resize(move |size| seen.set(size))
                    .w(100.0)
                    .h(100.0)
                    .child(div().size_full()),
            )
        });
        ui.drag_on("panel-resize-handle", end)?;
        assert_eq!(size.get(), 150.0);
        assert_eq!(seen.get(), 150.0);
        let point = ui.center_of("panel-resize-handle")?;
        ui.click(point);
        ui.click(point);
        ui.advance(0.0);
        assert_eq!(size.get(), 90.0);
        assert_eq!(seen.get(), 90.0);
        size.set(25.0);
        ui.advance(0.0);
        assert_eq!(size.get(), 50.0);
    }
    Ok(())
}
