mod common;

use common::builder;
use rouge_core::testing::TestResult;
use rouge_core::{Point, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{Side, resizable_panel};

#[test]
fn panels_resize_in_rem_with_correct_direction_clamp_and_reset_on_double_click() -> TestResult {
    for (side, end) in [
        (Side::Left, Point::new(250.0, 50.0)),
        (Side::Right, Point::new(-100.0, 50.0)),
        (Side::Top, Point::new(50.0, 250.0)),
        (Side::Bottom, Point::new(50.0, -100.0)),
    ] {
        let _entered = Runtime::new().enter();
        let (size, seen, resets) = (signal(6.0), signal(0.0), signal(0));
        let mut ui = builder().build(|| {
            div().size_full().child(
                resizable_panel(side, size)
                    .min_size(4.0)
                    .max_size(8.0)
                    .reset_size(5.0)
                    .on_resize(move |size| seen.set(size))
                    .on_reset(move || resets.update(|n| *n += 1))
                    .w(100.0)
                    .h(100.0)
                    .child(div().size_full()),
            )
        });
        ui.drag_on("panel-resize-handle", end)?;
        assert_eq!((size.get(), seen.get()), (8.0, 8.0));
        let point = ui.center_of("panel-resize-handle")?;
        ui.click(point);
        ui.click(point);
        ui.advance(0.0);
        assert_eq!((size.get(), seen.get(), resets.get()), (5.0, 5.0, 1));
        size.set(1.0);
        ui.advance(0.0);
        assert_eq!(size.get(), 4.0);
    }
    Ok(())
}
