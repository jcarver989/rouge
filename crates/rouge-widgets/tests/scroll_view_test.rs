mod common;

use common::builder;
use rouge_core::testing::{TestError, TestResult, TestUi, WHITE};
use rouge_core::{Point, ScrollState, Scrollable, Styled, div, scroll, virtual_list};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::scroll_view;

#[test]
fn scroll_view_shows_a_thumb_that_tracks_the_offset_only_when_content_overflows() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| {
        div()
            .size_full()
            .flex()
            .child(
                scroll_view(scroll().child(div().h(400.0)))
                    .w(100.0)
                    .h(100.0),
            )
            .child(scroll_view(scroll().child(div().h(50.0))).w(100.0).h(100.0))
    });
    ui.run(0.6);
    let thumb = |ui: &mut TestUi| -> Result<Vec<u32>, TestError> {
        let image = ui.render()?;
        Ok((0..100).filter(|y| image.pixel(95, *y) != WHITE).collect())
    };
    let rows = thumb(&mut ui)?;
    assert_eq!(
        (rows.first(), rows.len()),
        (Some(&0), 25),
        "a quarter of the viewport, at the top"
    );
    let image = ui.render()?;
    assert!(
        (0..100).all(|y| image.pixel(195, y) == WHITE),
        "no thumb without overflow"
    );
    let passes = ui.stats();
    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll(Point::new(0.0, -300.0));
    ui.run(0.6);
    let rows = thumb(&mut ui)?;
    assert_eq!(
        (rows.first(), rows.len()),
        (Some(&75), 25),
        "at the end of its travel"
    );
    assert_eq!(
        ui.stats().layouts,
        passes.layouts,
        "scrolling moves the thumb without layout"
    );
    Ok(())
}

#[test]
fn scroll_view_wraps_a_virtual_list_and_shares_its_scroll_state() -> TestResult {
    let _entered = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = builder().build(move || {
        let rows = virtual_list(|| 100, 20.0, |_| div().size_full());
        scroll_view(rows).scroll_state(state).w(100.0).h(100.0)
    });
    ui.run(0.6);
    let thumb = |ui: &mut TestUi| -> Result<Vec<u32>, TestError> {
        let image = ui.render()?;
        Ok((0..100).filter(|y| image.pixel(95, *y) != WHITE).collect())
    };
    let rows = thumb(&mut ui)?;
    assert_eq!(
        (rows.first(), rows.len()),
        (Some(&0), 24),
        "the minimum thumb, at the top"
    );
    assert_eq!(state.get().content.height, 2000.0);
    state.update(|state| state.offset.y = 1900.0);
    ui.run(0.6);
    let rows = thumb(&mut ui)?;
    assert_eq!(
        (rows.first(), rows.len()),
        (Some(&76), 24),
        "writing the shared state scrolls the list and moves the thumb"
    );
    Ok(())
}
