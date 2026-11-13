mod common;

use common::builder;
use rouge_core::testing::{TestResult, assert_near};
use rouge_core::{Color, Rect, Styled, UiPalette, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{progress, separator, spinner};

#[test]
fn progress_fills_in_proportion_and_separators_draw_a_line() -> TestResult {
    let _entered = Runtime::new().enter();
    let done = signal(0.25f32);
    let mut ui = builder().build(move || {
        div()
            .size_full()
            .p(10.0)
            .flex()
            .col()
            .gap(10.0)
            .child(progress(done).w(200.0))
            .child(separator(false))
    });
    let palette = UiPalette::light().map(Color::from);
    let img = ui.render()?;
    assert_near(
        img.pixel(40, 13),
        palette.accent.to_rgba8(),
        "inside the first quarter",
    );
    assert_near(
        img.pixel(100, 13),
        palette.border.to_rgba8(),
        "the rest of the track",
    );
    assert_near(
        img.pixel(150, 26),
        palette.border.to_rgba8(),
        "the separator spans the row",
    );
    done.set(2.0);
    ui.advance(0.0);
    assert_near(
        ui.render()?.pixel(200, 13),
        palette.accent.to_rgba8(),
        "values are clamped to full",
    );
    Ok(())
}

#[test]
fn vertical_separators_span_the_height_of_a_row() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| div().size_full().flex().child(separator(true)));
    assert_eq!(ui.bounds(0.5, 100.0)?, Rect::new(0.0, 0.0, 1.0, 200.0));
    Ok(())
}

#[test]
fn spinner_animates_in_the_compositor_without_layout_or_paint() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| div().size_full().p(10.0).child(spinner()));
    ui.run(0.6);
    let before = ui.stats();
    let first = ui.render()?;
    assert!(ui.is_animating());
    ui.advance(0.2);
    let after = ui.stats();
    assert_eq!(
        (before.layouts, before.paints),
        (after.layouts, after.paints)
    );
    let second = ui.render()?;
    let moved = (10..30).any(|y| (10..30).any(|x| first.pixel(x, y) != second.pixel(x, y)));
    assert!(moved, "the arc rotated");
    Ok(())
}
