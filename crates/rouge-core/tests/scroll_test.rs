use rouge_core::testing::{
    BLACK, BLUE, GREEN, RED, TestResult, TestUi, TestUiBuilder, WHITE, counter,
};
use rouge_core::{
    Align, Color, Point, RowKey, ScrollEvent, ScrollLinked, ScrollState, Scrollable, Size, Styled,
    View, div, dynamic, rem, root_font_size, scroll, text, virtual_list, virtual_list_handle,
};
use rouge_reactive::{Runtime, Signal, batch, effect, on_cleanup, signal};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

#[test]
fn dynamic_rebuild_preserves_virtual_list_and_ordinary_scroll_offsets() -> TestResult {
    let _rt = Runtime::new().enter();
    let version = signal(false);
    let state = signal(ScrollState::default());
    let (mut ui, handle) = TestUiBuilder::default().size(220.0, 100.0).build_with(|| {
        let handle = virtual_list_handle();
        let branch = dynamic(move || {
            let color = if version.get() {
                Color::RED
            } else {
                Color::BLUE
            };
            div()
                .size_full()
                .flex()
                .child(
                    virtual_list(|| 100, 20.0, move |_| div().size_full().bg(color))
                        .handle(handle)
                        .w(100.0)
                        .h_full(),
                )
                .child(
                    scroll()
                        .scroll_state(state)
                        .w(100.0)
                        .h_full()
                        .child(div().h(1000.0).bg(color)),
                )
        });
        (div().size_full().child(branch), handle)
    });
    handle.scroll_to(Point::new(0.0, 435.0));
    state.update(|state| state.scroll_to(Point::new(0.0, 250.0)));
    ui.advance(0.0);
    assert_eq!(handle.scroll_state().get().offset.y, 435.0);
    assert_eq!(state.get().offset.y, 250.0);
    version.set(true);
    assert_eq!(
        handle.row_top(30),
        0.0,
        "disposed row memos are not read between builds"
    );
    ui.advance(0.0);
    assert_eq!(handle.scroll_state().get().offset.y, 435.0);
    assert_eq!(handle.top_row().get(), 22);
    assert_eq!(state.get().offset.y, 250.0);
    assert_eq!(handle.row_top(30), 600.0);
    assert!(handle.row_bounds(22).is_some());
    handle.scroll_to_row(30, Align::Top);
    ui.advance(0.0);
    assert_eq!(handle.scroll_state().get().offset.y, 600.0);
    Ok(())
}

#[test]
fn scrolling_moves_content_in_the_compositor_and_clamps() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = scroller(signal(ScrollState::default()));
    assert_eq!(ui.pixel(50, 50)?, RED);
    let stats = ui.stats();

    ui.scroll(Point::new(0.0, -60.0));
    assert!(
        !ui.needs_frame(),
        "nothing to scroll without a pointer over it"
    );
    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll(Point::new(0.0, -60.0));
    assert!(ui.needs_frame());
    ui.update();
    let img = ui.render()?;
    assert_eq!((img.pixel(50, 30), img.pixel(50, 50)), (RED, BLUE));
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "no layout, no paint, no layer repainted"
    );

    ui.scroll(Point::new(0.0, -1000.0));
    ui.update();
    let img = ui.render()?;
    assert_eq!(
        (img.pixel(50, 10), img.pixel(50, 90)),
        (GREEN, GREEN),
        "clamped at the end"
    );
    ui.scroll(Point::new(0.0, 5000.0));
    ui.update();
    assert_eq!(ui.pixel(50, 90)?, RED, "and at the start");
    Ok(())
}

#[test]
fn scrolled_content_is_clipped_to_the_container_but_its_own_paint_stays_put() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        let container = scroll()
            .absolute()
            .left(20.0)
            .top(20.0)
            .size(50.0)
            .p(5.0)
            .bg(Color::BLACK);
        div().size_full().child(container.child(bands()))
    });
    ui.pointer_moved(Point::new(40.0, 40.0));
    ui.scroll(Point::new(0.0, -30.0));
    ui.update();
    let img = ui.render()?;
    assert_eq!(img.pixel(40, 40), RED);
    assert_eq!(
        img.pixel(40, 10),
        WHITE,
        "content does not leak above the container"
    );
    assert_eq!(img.pixel(40, 80), WHITE, "or below it");
    assert_eq!(
        img.pixel(22, 40),
        BLACK,
        "the container's own background does not scroll"
    );
    Ok(())
}

#[test]
fn hit_testing_follows_scrolled_content() {
    let _rt = Runtime::new().enter();
    let clicked = signal(Vec::new());
    let mut ui = TestUi::canvas(move || {
        let row = move |i: u32| {
            div()
                .h(50.0)
                .on_click(move |_| clicked.update(|c| c.push(i)))
        };
        div()
            .size_full()
            .child(scroll().w(100.0).h(100.0).children((0..6).map(row)))
    });
    ui.click(Point::new(10.0, 10.0));
    ui.pointer_moved(Point::new(10.0, 10.0));
    ui.scroll(Point::new(0.0, -120.0));
    ui.click(Point::new(10.0, 10.0));
    ui.click(Point::new(10.0, 90.0));
    ui.click(Point::new(150.0, 10.0));
    assert_eq!(clicked.get_untracked(), [0, 2, 4]);
}

#[test]
fn a_scroller_at_its_limit_hands_the_wheel_to_its_ancestor() {
    let _rt = Runtime::new().enter();
    let outer = signal(ScrollState::default());
    let inner = signal(ScrollState::default());
    let mut ui = TestUi::canvas(move || {
        let nested = scroll().h(50.0).scroll_state(inner).child(div().h(80.0));
        scroll()
            .size_full()
            .scroll_state(outer)
            .child(nested)
            .child(div().h(200.0))
    });
    let offsets = || {
        (
            inner.get_untracked().offset.y,
            outer.get_untracked().offset.y,
        )
    };
    ui.pointer_moved(Point::new(10.0, 10.0));
    ui.scroll(Point::new(0.0, -20.0));
    ui.update();
    assert_eq!(offsets(), (20.0, 0.0));
    ui.scroll(Point::new(0.0, -20.0));
    ui.update();
    assert_eq!(offsets(), (30.0, 0.0));
    ui.scroll(Point::new(0.0, -20.0));
    ui.update();
    assert_eq!(offsets(), (30.0, 20.0));
}

#[test]
fn scroll_handlers_can_veto_the_default_scroll() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        let container = scroll()
            .w(100.0)
            .h(100.0)
            .on_scroll(ScrollEvent::stop_propagation);
        div().size_full().child(container.child(bands()))
    });
    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll(Point::new(0.0, -60.0));
    ui.update();
    assert_eq!(ui.pixel(50, 50)?, RED);
    Ok(())
}

#[test]
fn scroll_state_reports_offset_and_extents_once_per_frame() {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let (runs, bump) = counter();
    let mut ui = TestUi::canvas(move || {
        effect(move || {
            state.get();
            bump();
        });
        div().size_full().child(
            scroll()
                .w(100.0)
                .h(80.0)
                .border(2.0)
                .scroll_state(state)
                .child(bands()),
        )
    });
    let seen = state.get_untracked();
    assert_eq!(seen.viewport, Size::new(96.0, 76.0), "the padding box");
    assert_eq!(seen.content, Size::new(96.0, 300.0));
    assert_eq!(runs.get(), 2, "initial run, then one update after layout");

    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll(Point::new(0.0, -10.0));
    ui.scroll(Point::new(0.0, -10.0));
    ui.scroll(Point::new(0.0, -10.0));
    assert_eq!(
        state.get_untracked().offset,
        Point::ZERO,
        "published with the frame"
    );
    ui.update();
    assert_eq!(state.get_untracked().offset, Point::new(0.0, 30.0));
    assert_eq!(runs.get(), 3, "three wheel events, one update");
}

#[test]
fn virtual_list_mounts_only_the_rows_near_the_viewport() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, mounted, _) = rows(100_000);
    assert_eq!(
        *mounted.borrow(),
        (0..16).collect(),
        "viewport plus overscan"
    );
    assert_eq!(ui.pixel(10, 5)?, RED);
    assert_eq!(ui.pixel(10, 25)?, BLUE);
    let elements = ui.stats().elements;

    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll(Point::new(0.0, -40.0));
    ui.update();
    assert_eq!(
        *mounted.borrow(),
        (0..16).collect(),
        "small scrolls stay inside the overscan"
    );
    let passes = ui.passes();
    ui.scroll(Point::new(0.0, -20.0));
    ui.update();
    assert_eq!(ui.passes(), passes, "and cost no layout or paint");

    ui.scroll(Point::new(0.0, -1_000_000.0 + 60.0));
    ui.update();
    assert_eq!(
        *mounted.borrow(),
        (49_992..50_016).collect(),
        "rows far away were swapped in"
    );
    assert!(ui.stats().elements <= elements + 16, "the tree stays small");
    let img = ui.render()?;
    assert_eq!(
        (img.pixel(10, 5), img.pixel(10, 25)),
        (RED, BLUE),
        "row 50000 is at the top"
    );

    ui.scroll(Point::new(0.0, -10_000_000.0));
    ui.update();
    assert_eq!(
        mounted.borrow().last(),
        Some(&99_999),
        "clamped to the last row"
    );
    assert_eq!(ui.pixel(10, 95)?, BLUE, "which sits at the bottom edge");
    Ok(())
}

#[test]
fn virtual_list_follows_its_count() {
    let _rt = Runtime::new().enter();
    let (mut ui, mounted, count) = rows(3);
    assert_eq!(*mounted.borrow(), (0..3).collect());
    count.set(5);
    ui.update();
    assert_eq!(*mounted.borrow(), (0..5).collect());
    count.set(1);
    ui.update();
    assert_eq!(*mounted.borrow(), (0..1).collect());
}

#[test]
fn virtual_list_places_rows_of_differing_heights() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, state) = sections();
    assert_eq!(
        state.get_untracked().content.height,
        100.0 * 40.0 + 900.0 * 20.0,
        "the content is as tall as every row together"
    );
    let img = ui.render()?;
    assert_eq!(
        (img.pixel(10, 35), img.pixel(10, 45)),
        (RED, BLUE),
        "the header row is twice as tall as the rows after it"
    );

    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll(Point::new(0.0, -(40.0 + 9.0 * 20.0) * 50.0));
    ui.update();
    let img = ui.render()?;
    assert_eq!(
        (img.pixel(10, 35), img.pixel(10, 45)),
        (RED, BLUE),
        "the fiftieth header is found by its offset, not its index"
    );
    Ok(())
}

#[test]
fn writing_the_scroll_state_scrolls_to_that_offset() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, state) = sections();
    state.update(|state| state.offset.y = 200.0);
    ui.update();
    assert_eq!(state.get_untracked().offset, Point::new(0.0, 200.0));
    let img = ui.render()?;
    assert_eq!(
        (img.pixel(10, 15), img.pixel(10, 25)),
        (BLUE, RED),
        "the second header sits one row below the top"
    );

    state.update(|state| state.offset.y = 1.0e9);
    ui.update();
    let state = state.get_untracked();
    assert_eq!(
        state.offset.y,
        state.content.height - state.viewport.height,
        "an offset past the end is clamped"
    );
    Ok(())
}

#[test]
fn an_offset_written_before_the_content_grows_lands_after_the_next_layout() {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let height = signal(150.0);
    let mut ui = TestUi::canvas(move || {
        let container = scroll().w(100.0).h(100.0).scroll_state(state);
        div().size_full().child(container.child(div().h(height)))
    });
    ui.update();
    height.set(400.0);
    state.update(|state| state.offset.y = 250.0);
    ui.update();
    assert_eq!(
        state.get_untracked().offset.y,
        250.0,
        "the target is clamped against the new extent, not the old one"
    );
    state.update(|state| state.offset.y = 100.0);
    ui.update();
    height.set(900.0);
    ui.update();
    assert_eq!(
        state.get_untracked().offset.y,
        100.0,
        "a target that already landed is not re-applied by later layouts"
    );
}

#[test]
fn virtual_list_reports_the_rows_in_view() {
    let _rt = Runtime::new().enter();
    let visible = signal(0..0);
    let mut ui = TestUi::canvas(move || {
        let list = virtual_list(|| 1000, 20.0, |_| div().size_full())
            .track_rows(visible)
            .w(100.0)
            .h(100.0);
        div().size_full().child(list)
    });
    ui.update();
    assert_eq!(visible.get_untracked(), 0..5);
    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll(Point::new(0.0, -50.0));
    ui.update();
    assert_eq!(visible.get_untracked(), 2..8, "partly visible rows count");
}

#[test]
#[expect(clippy::similar_names)]
fn dragged_scrolling_rubber_bands_then_springs_back() -> TestResult {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = scroller(state);
    ui.render()?;
    let stats = ui.stats();
    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll_drag(Point::new(0.0, 30.0));
    ui.advance(0.0);
    assert_eq!(state.get_untracked().offset.y, -30.0, "pulled past the top");
    let img = ui.render()?;
    assert_eq!(
        (img.pixel(50, 10), img.pixel(50, 40)),
        (WHITE, RED),
        "the content follows the finger"
    );
    assert!(!ui.needs_frame(), "and holds while it is held");

    ui.scroll_release(Point::ZERO);
    assert!(ui.needs_frame());
    ui.run(3.0);
    assert!(
        state.get_untracked().offset.y.abs() < 0.01,
        "sprung back to the top"
    );
    assert_eq!(ui.pixel(50, 10)?, RED);
    assert!(!ui.needs_frame());
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "no layout and no paint"
    );
    Ok(())
}

#[test]
fn releasing_a_scroll_with_velocity_keeps_momentum() {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = scroller(state);
    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll_drag(Point::new(0.0, -20.0));
    ui.scroll_release(Point::new(0.0, -200.0));
    ui.run(0.1);
    let coasting = state.get_untracked().offset.y;
    assert!(
        coasting > 30.0 && coasting < 60.0,
        "still moving after release: {coasting}"
    );
    ui.run(5.0);
    let rest = state.get_untracked().offset.y;
    assert!(
        (rest - 119.5).abs() < 0.2,
        "20 dragged plus 200/2 of momentum: {rest}"
    );
    assert!(!ui.needs_frame());

    ui.scroll_drag(Point::new(0.0, -20.0));
    ui.scroll_release(Point::new(0.0, -4000.0));
    ui.run(5.0);
    assert_eq!(
        state.get_untracked().offset.y,
        200.0,
        "momentum past the end bounces back onto it"
    );
}

#[test]
fn list_handle_scrolls_to_each_alignment_and_exposes_mounted_bounds() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, handle) = TestUiBuilder::default().build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(|| 100, 20.0, |_| div().size_full())
                .handle(handle)
                .w(100.0)
                .h(100.0),
            handle,
        )
    });
    for (align, offset, top) in [
        (Align::Top, 400.0, 0.0),
        (Align::Center, 360.0, 40.0),
        (Align::Bottom, 320.0, 80.0),
    ] {
        handle.scroll_to_row(20, align);
        ui.advance(0.0);
        assert_eq!(handle.scroll_state().get().offset.y, offset);
        let bounds = handle.row_bounds(20).ok_or("row is mounted")?;
        assert_eq!(bounds.origin.y, top);
        assert_eq!(ui.bounds_at(bounds.center()), Some(bounds));
    }
    handle.scroll_to_row(19, Align::Visible);
    ui.advance(0.0);
    assert_eq!(handle.scroll_state().get().offset.y, 320.0);
    handle.scroll_to_row(10, Align::Visible);
    ui.advance(0.0);
    assert_eq!(handle.scroll_state().get().offset.y, 200.0);
    handle.scroll_to_row(20, Align::Visible);
    ui.advance(0.0);
    assert_eq!(handle.scroll_state().get().offset.y, 320.0);
    assert_eq!(handle.row_top(20), 400.0);
    assert_eq!(handle.row_bounds(99), None);
    handle.scroll_to_row(1000, Align::Top);
    ui.advance(0.0);
    assert_eq!(handle.scroll_state().get().offset.y, 1900.0);
    Ok(())
}

#[test]
fn list_reveal_leaves_space_for_an_overlay_at_the_top() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, handle) = TestUiBuilder::default().build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(|| 100, 20.0, |_| div().h(20.0))
                .measure_rows(|| 0)
                .handle(handle)
                .w(100.0)
                .h(100.0),
            handle,
        )
    });
    handle.scroll_to_row(20, Align::Top);
    ui.advance(0.0);
    handle.reveal_row(20, 32.0);
    ui.advance(0.0);
    assert_eq!(
        handle.row_bounds(20).ok_or("row is mounted")?.origin.y,
        32.0
    );
    handle.reveal_row(80, 32.0);
    ui.advance(0.0);
    assert_eq!(
        handle.row_bounds(80).ok_or("row is mounted")?.origin.y,
        80.0
    );
    handle.reveal_row(0, 32.0);
    ui.advance(0.0);
    assert_eq!(handle.row_bounds(0).ok_or("row is mounted")?.origin.y, 0.0);
    Ok(())
}

#[test]
fn top_row_reports_the_first_fully_visible_row_once_after_wheel_scrolling() {
    let _rt = Runtime::new().enter();
    let (mut ui, handle) = TestUiBuilder::default().build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(|| 100, 20.0, |_| div())
                .handle(handle)
                .size_full(),
            handle,
        )
    });
    let top = handle.top_row();
    let (runs, bump) = counter();
    effect(move || {
        top.get();
        bump();
    });
    ui.pointer_moved(Point::new(10.0, 10.0));
    ui.scroll(Point::new(0.0, -20.0));
    ui.scroll(Point::new(0.0, -21.0));
    assert_eq!(top.get(), 0);
    ui.advance(0.0);
    assert_eq!(top.get(), 3);
    assert_eq!(runs.get(), 2);
}

#[test]
fn reactive_row_heights_only_recompute_affected_inputs_and_preserve_the_anchor() {
    let _rt = Runtime::new().enter();
    let first_height = signal(20.0);
    let (runs, bump) = counter();
    let (mut ui, handle) = TestUiBuilder::default().build_with(|| {
        let handle = virtual_list_handle();
        let height = move |index| {
            bump();
            if index == 0 { first_height.get() } else { 20.0 }
        };
        (
            virtual_list(|| 100, height, |_| div().size_full())
                .handle(handle)
                .size_full(),
            handle,
        )
    });
    assert_eq!(runs.get(), 100);
    handle.scroll_to(Point::new(0.0, 405.0));
    ui.advance(0.0);
    assert_eq!(handle.top_row().get(), 21);
    first_height.set(40.0);
    ui.advance(0.0);
    assert_eq!(runs.get(), 101);
    assert_eq!(handle.top_row().get(), 21);
    assert_eq!(handle.scroll_state().get().offset.y, 425.0);
    assert_eq!(handle.row_top(20), 420.0);
}

#[test]
fn rows_from_a_superseded_presentation_are_not_mounted() {
    let _rt = Runtime::new().enter();
    let count = signal(1);
    let mut ui = TestUi::canvas(move || {
        virtual_list(
            move || count.get(),
            20.0,
            move |index| {
                assert!(index < count.get_untracked());
                div().size_full()
            },
        )
        .row_identity(|index| RowKey::new(index, 0))
        .size_full()
    });
    count.set(100);
    count.set(1);
    ui.update();
}

#[test]
fn keyed_rows_refresh_the_view_and_pointer_target_at_the_same_index() -> TestResult {
    let _rt = Runtime::new().enter();
    let replacement = signal(false);
    let clicked = signal(None);
    let mut ui = TestUi::canvas(move || {
        virtual_list(
            || 100,
            20.0,
            move |index| {
                if replacement.get_untracked() && (index == 0 || index == 50) {
                    div()
                        .size_full()
                        .bg(Color::BLUE)
                        .on_click(move |_| clicked.set(Some((index, true))))
                } else {
                    div()
                        .size_full()
                        .bg(Color::RED)
                        .on_click(move |_| clicked.set(Some((index, false))))
                }
            },
        )
        .row_identity(move |index| {
            let replaced = replacement.get() && (index == 0 || index == 50);
            RowKey::new(
                if replaced && index == 50 { 1000 } else { index },
                u64::from(replaced),
            )
        })
        .size_full()
    });
    assert_eq!(ui.pixel(10, 10)?, RED);
    replacement.set(true);
    ui.update();
    assert_eq!(ui.pixel(10, 10)?, BLUE);
    ui.click_at(10.0, 10.0);
    assert_eq!(clicked.get(), Some((0, true)));
    ui.scroll(Point::new(0.0, -1000.0));
    ui.update();
    assert_eq!(ui.pixel(10, 10)?, BLUE);
    ui.click_at(10.0, 10.0);
    assert_eq!(clicked.get(), Some((50, true)));
    Ok(())
}

#[test]
fn keyed_anchor_survives_reorder_and_falls_forward_when_removed() -> TestResult {
    let _rt = Runtime::new().enter();
    let ids = signal((0..100).collect::<Vec<_>>());
    let (mut ui, handle) = TestUiBuilder::default().size(100.0, 100.0).build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(move || ids.with(Vec::len), 20.0, |_| div().size_full())
                .row_identity(move |index| RowKey::new(ids.with(|ids| ids[index]), 0))
                .handle(handle)
                .size_full(),
            handle,
        )
    });
    handle.scroll_to(Point::new(0.0, 405.0));
    ui.update();
    ids.update(|ids| ids.splice(0..0, [100, 101]).for_each(drop));
    ui.update();
    assert_eq!(handle.scroll_state().get().offset.y, 445.0);
    ids.update(|ids| ids.swap(22, 40));
    ui.update();
    assert_eq!(
        handle
            .row_bounds(40)
            .ok_or("reordered anchor is mounted")?
            .origin
            .y,
        -5.0
    );
    ids.update(|ids| {
        ids.remove(40);
        ids.remove(40);
        ids.swap(40, 50);
    });
    ui.update();
    assert_eq!(
        handle
            .row_bounds(50)
            .ok_or("surviving successor is mounted")?
            .origin
            .y,
        -5.0
    );
    Ok(())
}

#[test]
fn indexed_estimates_use_the_current_presentation_and_keep_the_keyed_anchor() -> TestResult {
    let _rt = Runtime::new().enter();
    let model = signal((0..100).map(|id| (id, 20.0)).collect::<Vec<_>>());
    let (mut ui, handle) = TestUiBuilder::default().size(100.0, 100.0).build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(
                move || model.with(Vec::len),
                move |i: usize| model.with(|rows| rows[i].1),
                |_| div().size_full(),
            )
            .row_identity(move |index| RowKey::new(model.with(|rows| rows[index].0), 0))
            .handle(handle)
            .size_full(),
            handle,
        )
    });
    handle.scroll_to(Point::new(0.0, 405.0));
    ui.update();
    model.update(|rows| {
        rows.truncate(50);
        rows.insert(0, (100, 150.0));
    });
    ui.update();
    assert_eq!(handle.row_top(21), 550.0);
    assert_eq!(
        handle.row_bounds(21).ok_or("anchor is mounted")?.origin.y,
        -5.0
    );
    Ok(())
}

#[test]
fn measured_rows_keep_the_partial_anchor_and_align_using_actual_heights() -> TestResult {
    let _rt = Runtime::new().enter();
    let expanded = signal(false);
    let (mut ui, handle) = TestUiBuilder::default().size(100.0, 100.0).build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(
                || 100,
                20.0,
                move |index| {
                    div().h(move || match index {
                        19 if expanded.get() => 50.0,
                        70 => 60.0,
                        _ => 20.0,
                    })
                },
            )
            .measure_rows(|| 0)
            .handle(handle)
            .size_full(),
            handle,
        )
    });
    handle.scroll_to(Point::new(0.0, 405.0));
    ui.update();
    expanded.set(true);
    ui.update();
    assert_eq!(handle.row_top(20), 430.0);
    assert_eq!(handle.scroll_state().get().offset.y, 435.0);
    assert_eq!(
        handle.row_bounds(20).ok_or("anchor is mounted")?.origin.y,
        -5.0
    );
    handle.scroll_to_row(70, Align::Center);
    ui.update();
    let row = handle.row_bounds(70).ok_or("centered row is mounted")?;
    assert_eq!(row.size.height, 60.0);
    assert_eq!(row.origin.y, 20.0);
    handle.scroll_to_row(70, Align::Bottom);
    ui.update();
    assert_eq!(
        handle
            .row_bounds(70)
            .ok_or("bottom-aligned row is mounted")?
            .origin
            .y,
        40.0
    );
    assert!(
        !ui.update(),
        "measurement settles without keeping the list dirty"
    );
    Ok(())
}

#[test]
fn measured_list_reflows_wrapped_text_and_invalidates_offscreen_cache() {
    let _rt = Runtime::new().enter();
    let revision = signal(0);
    let (mut ui, (handle, font)) = TestUiBuilder::default().size(100.0, 100.0).build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(
                || 200,
                20.0,
                |index| {
                    if index == 0 {
                        text("This paragraph wraps to several lines at a narrow width")
                            .text_size(rem(1.0))
                            .into_any()
                    } else {
                        div().h(20.0).into_any()
                    }
                },
            )
            .row_identity(|index| RowKey::new(index, 0))
            .measure_rows(move || revision.get())
            .handle(handle)
            .size_full(),
            (handle, root_font_size()),
        )
    });
    let narrow = handle.row_top(1);
    assert!(narrow > 20.0);
    ui.set_size(Size::new(250.0, 100.0));
    ui.update();
    let wide = handle.row_top(1);
    assert!(wide < narrow);
    font.set(26.0);
    ui.update();
    assert!(handle.row_top(1) > wide);
    handle.scroll_to_row(100, Align::Top);
    ui.update();
    assert!(handle.row_bounds(0).is_none());
    assert!(handle.row_top(1) > 20.0);
    revision.set(1);
    ui.update();
    assert_eq!(handle.row_top(1), 20.0);
    assert_eq!(handle.scroll_state().get().offset.y, 2000.0);
    for width in [100.0, 250.0, 100.0] {
        ui.set_size(Size::new(width, 100.0));
        ui.update();
        assert_eq!(handle.scroll_state().get().offset.y, 2000.0);
    }
}

#[test]
fn offscreen_measurements_follow_identity_but_not_content_revision() {
    let _rt = Runtime::new().enter();
    let swapped = signal(false);
    let changed = signal(false);
    let index_id = move |index| match (swapped.get(), index) {
        (true, 0) => 5,
        (true, 5) => 0,
        _ => index,
    };
    let (mut ui, handle) = TestUiBuilder::default().size(100.0, 100.0).build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(
                || 200,
                20.0,
                move |index| {
                    let special = index_id(index) == 0;
                    div().h(if !special {
                        20.0
                    } else if changed.get_untracked() {
                        30.0
                    } else {
                        80.0
                    })
                },
            )
            .row_identity(move |index| {
                let id = index_id(index);
                RowKey::new(id, u64::from(id == 0 && changed.get()))
            })
            .measure_rows(|| 0)
            .handle(handle)
            .size_full(),
            handle,
        )
    });
    assert_eq!(handle.row_top(1), 80.0);
    handle.scroll_to_row(100, Align::Top);
    ui.update();
    assert!(handle.row_bounds(0).is_none());
    swapped.set(true);
    ui.update();
    assert_eq!(handle.row_top(1), 20.0);
    assert_eq!(handle.row_top(6), 180.0);
    assert!(handle.row_bounds(5).is_none());
    changed.set(true);
    ui.update();
    assert_eq!(handle.row_top(6), 120.0);
    assert_eq!(handle.top_row().get(), 100);
    handle.scroll_to_row(5, Align::Top);
    ui.update();
    assert_eq!(handle.row_top(6), 130.0);
    assert_eq!(handle.top_row().get(), 5);
}

#[test]
fn hidden_rows_and_zero_viewports_do_not_mount_offscreen_content() {
    let _rt = Runtime::new().enter();
    let count = signal(100_000);
    let visible = signal(0..0);
    let (mounts, bump) = counter();
    let (mut ui, handle) = TestUiBuilder::default().size(100.0, 100.0).build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(
                move || count.get(),
                |i| if i == 0 || i == 99_999 { 20.0 } else { 0.0 },
                move |_| {
                    bump();
                    div().size_full()
                },
            )
            .track_rows(visible)
            .handle(handle)
            .size_full(),
            handle,
        )
    });
    assert_eq!(mounts.get(), 2);
    assert_eq!(handle.scroll_state().get().content.height, 100.0);
    assert!(handle.row_bounds(1).is_none());
    ui.set_size(Size::new(100.0, 0.0));
    ui.update();
    assert!(visible.get().is_empty());
    assert!(handle.row_bounds(0).is_none());
    batch(|| {
        count.set(0);
        handle.scroll_to_row(10, Align::Top);
    });
    ui.set_size(Size::new(100.0, 100.0));
    ui.update();
    assert!(visible.get().is_empty());
    assert_eq!(handle.top_row().get(), 0);
    assert_eq!(handle.row_top(100), 0.0);
    assert_eq!(mounts.get(), 2);
}

#[test]
fn scrolling_a_large_measured_list_only_mounts_nearby_rows() {
    let _rt = Runtime::new().enter();
    let (identities, identify) = counter();
    let (mounts, mount) = counter();
    let (mut ui, handle) = TestUiBuilder::default().size(100.0, 100.0).build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(
                || 100_000,
                20.0,
                move |index| {
                    mount();
                    div().h(if index % 2 == 0 { 20.0 } else { 24.0 })
                },
            )
            .row_identity(move |index| {
                identify();
                RowKey::new(index, 0)
            })
            .measure_rows(|| 0)
            .handle(handle)
            .size_full(),
            handle,
        )
    });
    assert_eq!(identities.get(), 100_000);
    assert!(mounts.get() <= 24);
    let nodes = ui.stats().elements;
    for target in [50_000, 98_000, 20_000, 50_000] {
        let before = mounts.get();
        handle.scroll_to_row(target, Align::Top);
        ui.update();
        assert_eq!(handle.top_row().get(), target);
        assert!(mounts.get() - before <= 32);
        assert!(ui.stats().elements <= nodes + 32);
        assert!(handle.row_bounds(target).is_some());
    }
    assert_eq!(identities.get(), 100_000);
}

#[test]
fn sticky_layers_pin_within_their_range_of_the_scroll_offset() {
    let _rt = Runtime::new().enter();
    let sticky = ScrollLinked::Sticky {
        lo: 100.0,
        hi: 300.0,
    };
    assert_eq!(
        linked_y(sticky, [0.0, 200.0, 500.0]),
        [Some(100.0), Some(0.0), Some(-200.0)]
    );
}

#[test]
fn linear_links_translate_by_a_clamped_multiple_of_the_offset() {
    let _rt = Runtime::new().enter();
    let linear = ScrollLinked::Linear {
        factor: 0.25,
        max: 40.0,
        horizontal: false,
    };
    assert_eq!(
        linked_y(linear, [0.0, 100.0, 800.0]),
        [Some(0.0), Some(25.0), Some(40.0)]
    );
}

#[test]
fn scrolling_within_mounted_rows_does_no_layout_paint_or_building() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().size(200.0, 200.0).build(|| {
        virtual_list(
            || 1000,
            20.0,
            |index| text(format!("row {index}")).into_any(),
        )
        .size_full()
    });
    ui.pointer_moved(Point::new(100.0, 100.0));
    ui.advance(0.0);
    let mut quiet = 0;
    for _ in 0..60 {
        let before = ui.stats();
        ui.scroll(Point::new(0.0, -7.0));
        ui.advance(0.016);
        let frame = ui.stats() - before;
        if frame.elements_created + frame.elements_removed > 0 {
            continue;
        }
        quiet += 1;
        assert_eq!(
            (
                frame.layouts,
                frame.layer_paints,
                frame.reactive.nodes_created
            ),
            (0, 0, 0),
            "{frame:?}"
        );
        assert!(
            frame.reactive.effects_run + frame.reactive.memos_run <= 8,
            "{frame:?}"
        );
    }
    assert!(quiet > 30, "{quiet} quiet frames");
}

#[test]
fn a_following_list_sticks_to_its_end_until_scrolled_up_and_resumes_at_the_bottom() {
    let _rt = Runtime::new().enter();
    let (count, following) = (signal(20_usize), signal(true));
    let mut ui = TestUiBuilder::default().size(100.0, 100.0).build(move || {
        virtual_list(
            move || count.get(),
            20.0,
            |index| div().test_id(format!("row-{index}")).h(20.0),
        )
        .follow_end(following)
        .size_full()
    });
    let bottom = |ui: &mut TestUi, index: usize| {
        ui.advance(0.0);
        ui.bounds_of(&format!("row-{index}"))
            .map(|bounds| bounds.max().y)
    };
    assert_eq!(bottom(&mut ui, 19), Some(100.0));
    count.set(25);
    assert_eq!(bottom(&mut ui, 24), Some(100.0), "new rows stay in view");
    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll(Point::new(0.0, 60.0));
    ui.advance(0.0);
    assert!(!following.get(), "scrolling up stops following");
    count.set(30);
    assert_ne!(
        bottom(&mut ui, 29),
        Some(100.0),
        "and new rows no longer pull the list"
    );
    following.set(true);
    assert_eq!(bottom(&mut ui, 29), Some(100.0));
}

fn scroller(state: Signal<ScrollState>) -> TestUi {
    TestUi::canvas(move || {
        let container = scroll().w(100.0).h(100.0).scroll_state(state);
        div().size_full().child(container.child(bands()))
    })
}

fn bands() -> impl View {
    let band = |color| div().h(100.0).bg(color);
    div()
        .child(band(Color::RED))
        .child(band(Color::BLUE))
        .child(band(Color::GREEN))
}

fn rows(count: usize) -> (TestUi, Mounted, Signal<usize>) {
    let mounted = Mounted::default();
    let count = signal(count);
    let log = mounted.clone();
    let ui = TestUi::canvas(move || {
        let row = move |index: usize| {
            log.borrow_mut().insert(index);
            let log = log.clone();
            on_cleanup(move || {
                log.borrow_mut().remove(&index);
            });
            div().size_full().bg(if index.is_multiple_of(2) {
                Color::RED
            } else {
                Color::BLUE
            })
        };
        div().size_full().child(
            virtual_list(move || count.get(), 20.0, row)
                .w(100.0)
                .h(100.0),
        )
    });
    (ui, mounted, count)
}

fn sections() -> (TestUi, Signal<ScrollState>) {
    let state = signal(ScrollState::default());
    let ui = TestUi::canvas(move || {
        let header = |index: usize| index.is_multiple_of(10);
        let row = move |index: usize| {
            let color = if header(index) {
                Color::RED
            } else {
                Color::BLUE
            };
            div().size_full().bg(color)
        };
        let height = move |index: usize| if header(index) { 40.0 } else { 20.0 };
        div().size_full().child(
            virtual_list(|| 1000, height, row)
                .scroll_state(state)
                .w(100.0)
                .h(100.0),
        )
    });
    (ui, state)
}

type Mounted = Rc<RefCell<BTreeSet<usize>>>;

fn linked_y(link: ScrollLinked, offsets: [f32; 3]) -> [Option<f32>; 3] {
    let (mut ui, state) = TestUiBuilder::default().size(200.0, 100.0).build_with(|| {
        let state = signal(ScrollState::default());
        let view = div()
            .relative()
            .size_full()
            .child(
                div()
                    .test_id("linked")
                    .absolute()
                    .top(0.0)
                    .w(50.0)
                    .h(20.0)
                    .scroll_linked(state, link),
            )
            .child(
                scroll()
                    .size_full()
                    .scroll_state(state)
                    .child(div().h(1000.0)),
            );
        (view, state)
    });
    offsets.map(|offset| {
        state.update(|state| state.offset.y = offset);
        ui.advance(0.0);
        ui.bounds_of("linked").map(|bounds| bounds.origin.y)
    })
}
