mod common;

use common::builder;
use rouge_core::testing::{TestError, TestResult, TestUi, WHITE};
use rouge_core::view;
use rouge_core::{
    InputKind, Point, PointerButton, ScrollState, Scrollable, Styled, div, scroll, virtual_list,
};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::UNIT;
use rouge_widgets::{scroll_view, vertical_scrollbar};

#[test]
fn two_axis_thumbs_capture_drags_and_tracks_jump_without_cross_axis_changes() -> TestResult {
    let _entered = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = builder().build(|| {
        scroll_view(scroll().child(div().w(400.0).h(400.0)))
            .scroll_state(state)
            .w(100.0)
            .h(100.0)
    });
    ui.advance(0.0);
    ui.drag_on("scroll-thumb-vertical", Point::new(95.0, 50.0))?;
    ui.advance(0.0);
    assert!((state.get().offset.y - 150.0).abs() < 1.0);
    assert_eq!(state.get().offset.x, 0.0);
    ui.drag_on("scroll-thumb-horizontal", Point::new(900.0, 180.0))?;
    ui.advance(0.0);
    assert_eq!(state.get().offset.x, 300.0);
    assert!((state.get().offset.y - 150.0).abs() < 1.0);
    ui.click(Point::new(95.0, 80.0));
    ui.advance(0.0);
    assert!((state.get().offset.y - 282.35).abs() < 0.1);
    ui.pointer_moved(Point::new(95.0, 80.0));
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_moved(Point::new(200.0, -300.0));
    ui.pointer_up(PointerButton::Primary);
    ui.advance(0.0);
    assert_eq!(state.get().offset.y, 0.0);
    ui.pointer_moved(Point::new(20.0, 80.0));
    assert_eq!(state.get().offset.y, 0.0);
    Ok(())
}

#[test]
fn a_thumb_scrolled_to_the_end_meets_the_bottom_of_its_track() -> TestResult {
    let _entered = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = builder().build(|| {
        scroll_view(scroll().child(div().h(700.0)))
            .scroll_state(state)
            .w(100.0)
            .h(150.0)
    });
    ui.advance(0.0);
    state.update(|state| state.scroll_to(Point::new(0.0, f32::MAX)));
    ui.advance(0.0);
    let track = ui.rect_of("scroll-track-vertical")?;
    let thumb = ui.rect_of("scroll-thumb-vertical")?;
    assert_eq!(thumb.max().y, track.max().y);
    Ok(())
}

#[test]
fn a_thumb_is_a_whole_number_of_units_long_whatever_the_content() -> TestResult {
    for content in [321.0, 410.0, 450.0, 999.0] {
        let _entered = Runtime::new().enter();
        let mut ui = builder().build(move || {
            scroll_view(scroll().child(div().h(content)))
                .w(100.0)
                .h(150.0)
        });
        ui.advance(0.0);
        let thumb = ui.rect_of("scroll-thumb-vertical")?;
        assert_eq!(
            thumb.size.height % UNIT,
            0.0,
            "{content} of content gives {thumb:?}"
        );
    }
    Ok(())
}

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
        (Some(&0), 32),
        "the minimum thumb, at the top"
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
        (Some(&68), 32),
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
        (Some(&0), 32),
        "the minimum thumb, at the top"
    );
    assert_eq!(state.get().content.height, 2000.0);
    state.update(|state| state.offset.y = 1900.0);
    ui.run(0.6);
    let rows = thumb(&mut ui)?;
    assert_eq!(
        (rows.first(), rows.len()),
        (Some(&68), 32),
        "writing the shared state scrolls the list and moves the thumb"
    );
    Ok(())
}

#[test]
fn overflow_shows_a_thumb_that_tracks_only_its_own_list() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, (state, other)) = component_test().size(360.0, 200.0).build_with(|| {
        let state = signal(ScrollState::default());
        let other = signal(ScrollState::default());
        let panel = |state, height| {
            div()
                .relative()
                .w(160.0)
                .h(160.0)
                .child(
                    scroll()
                        .size_full()
                        .scroll_state(state)
                        .child(div().h(height)),
                )
                .child(vertical_scrollbar(state))
        };
        (
            div()
                .flex()
                .child(panel(state, 900.0))
                .child(panel(other, 60.0)),
            (state, other),
        )
    });
    assert_eq!(ui.bounds_of_all("scroll-thumb-vertical").len(), 1);
    let thumb = ui.center_of("scroll-thumb-vertical")?;
    ui.click_at(thumb.x, 90.0);
    assert!(state.get().offset.y > 0.0);
    ui.drag_on("scroll-thumb-vertical", Point::new(thumb.x, 180.0))?;
    assert!((state.get().offset.y - 740.0).abs() < 1.0);
    assert!(other.get().offset.y.abs() < 0.01);
    Ok(())
}

#[test]
fn pulling_past_the_top_starts_a_refresh_that_the_caller_ends() -> TestResult {
    let _entered = Runtime::new().enter();
    let refreshing = signal(false);
    let mut ui = component_test()
        .size(300.0, 400.0)
        .input(InputKind::Touch)
        .build(move || {
            view! {
                <scroll_view refresh=refreshing size_full>
                    <scroll>
                        <div h=2000.0/>
                    </scroll>
                </scroll_view>
            }
        });
    ui.swipe(Point::new(150.0, 100.0), Point::new(150.0, 120.0));
    ui.run(2.0);
    assert!(!refreshing.get(), "a short pull springs back");
    assert_eq!(ui.bounds_of("refresh-indicator"), None);
    ui.swipe(Point::new(150.0, 100.0), Point::new(150.0, 300.0));
    assert!(refreshing.get(), "pulled past the threshold");
    ui.run(2.0);
    assert!(
        ui.bounds_of("refresh-spinner").is_some(),
        "the spinner stays while refreshing"
    );
    refreshing.set(false);
    ui.run(2.0);
    assert_eq!(ui.bounds_of("refresh-indicator"), None);
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}
