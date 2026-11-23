use rouge_core::Wake;
use rouge_core::testing::{
    BLACK, BLUE, GREEN, RED, TestResult, TestUi, TestUiBuilder, WHITE, counter,
};
use rouge_core::view;
use rouge_core::{
    Align, Color, Point, RowKey, ScrollEvent, ScrollLinked, ScrollState, Scrollable, Size, Styled,
    View, VirtualListHandle, div, dynamic, scroll, text, virtual_list, virtual_list_handle,
    virtual_row_index,
};
use rouge_reactive::{KeyedList, Runtime, Signal, batch, effect, keyed_list, on_cleanup, signal};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;

#[test]
fn a_virtual_list_underlay_sits_behind_the_rows_and_scrolls_with_them() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().size(100.0, 100.0).build(|| {
        let underlay = view! {
            <div test_id="underlay" absolute left=0.0 top=40.0 w=100.0 h=20.0 bg=Color::RED/>
        };
        view! {
            <virtual_list count={|| 100} row_height=20.0 let:index underlay=underlay size_full>
                <div test_id={format!("row-{index}")} h=20.0/>
            </virtual_list>
        }
    });
    assert_eq!(ui.rect_of("underlay")?, ui.rect_of("row-2")?);
    assert_eq!(ui.pixel(50, 50)?, RED, "transparent rows show it");
    ui.scroll_on("row-2", Point::new(0.0, -20.0))?;
    ui.run(0.5);
    assert_eq!(ui.rect_of("underlay")?, ui.rect_of("row-2")?);
    assert_eq!(ui.rect_of("underlay")?.origin.y, 20.0);
    Ok(())
}

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
    assert_eq!(
        ui.next_frame(),
        Wake::Idle,
        "nothing to scroll without a pointer over it"
    );
    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll(Point::new(0.0, -60.0));
    assert_eq!(ui.next_frame(), Wake::Now);
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
    assert_eq!(ui.next_frame(), Wake::Idle, "and holds while it is held");

    ui.scroll_release();
    assert_eq!(ui.next_frame(), Wake::Now);
    ui.run(3.0);
    assert!(
        state.get_untracked().offset.y.abs() < 0.01,
        "sprung back to the top"
    );
    assert_eq!(ui.pixel(50, 10)?, RED);
    assert_eq!(ui.next_frame(), Wake::Idle);
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "no layout and no paint"
    );
    Ok(())
}

#[test]
fn releasing_a_scroll_drag_keeps_the_finger_momentum() {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = scroller(state);
    ui.pointer_moved(Point::new(50.0, 50.0));
    flick(&mut ui, 200.0, 0.05);
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
    assert_eq!(ui.next_frame(), Wake::Idle);

    flick(&mut ui, 4000.0, 0.0025);
    ui.run(5.0);
    assert_eq!(
        state.get_untracked().offset.y,
        200.0,
        "momentum past the end bounces back onto it"
    );
}

#[test]
fn a_flick_into_the_end_bounces_past_it_once_and_settles_without_more_input() {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = scroller(state);
    ui.pointer_moved(Point::new(50.0, 50.0));
    for _ in 0..5 {
        ui.scroll_drag(Point::new(0.0, -30.0));
        ui.advance(FRAME);
    }
    ui.scroll_release();
    let mut trace = Vec::new();
    for _ in 0..60 {
        ui.advance(FRAME);
        trace.push(state.get_untracked().offset.y);
    }
    let peak = trace.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        peak > 220.0 && peak < 260.0,
        "the finger's speed carries it past the end: {peak}"
    );
    let at_peak = trace.iter().position(|y| *y == peak).unwrap_or_default();
    assert!(
        at_peak < 10 && trace[at_peak + 10] < peak - 15.0,
        "and it turns straight back instead of being held: {trace:?}"
    );
    assert!(
        (state.get_untracked().offset.y - 200.0).abs() < 0.5,
        "settled on the end within a second: {trace:?}"
    );
    ui.run(1.0);
    assert_eq!(state.get_untracked().offset.y, 200.0);
    assert_eq!(ui.next_frame(), Wake::Idle);
}

#[test]
fn a_scroll_drag_held_still_before_lifting_does_not_coast() {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = scroller(state);
    ui.pointer_moved(Point::new(50.0, 50.0));
    for _ in 0..5 {
        ui.scroll_drag(Point::new(0.0, -10.0));
        ui.advance(FRAME);
    }
    ui.advance(0.2);
    ui.scroll_release();
    ui.run(1.0);
    assert_eq!(
        state.get_untracked().offset.y,
        50.0,
        "stays where it was let go"
    );
    assert_eq!(ui.next_frame(), Wake::Idle);
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
fn keyed_edits_keep_mounted_rows_their_focus_and_a_partial_anchor() -> TestResult {
    let _rt = Runtime::new().enter();
    let KeyedRows {
        mut ui,
        model,
        handle,
        mounts,
        pressed,
    } = keyed_rows(0..100).build();
    handle.scroll_to(Point::new(0.0, 405.0));
    ui.update();
    ui.click_on("id-22")?;
    let before = mounts.get();
    model.prepend((Id(100), 20.0));
    ui.update();
    assert_eq!(mounts.get() - before, 1);
    ui.press("enter");
    assert_eq!(pressed.get(), Some((22, 23)));
    batch(|| {
        model.move_to(&Id(22), 21);
        model.remove(&Id(0));
        model.update(&Id(22), |row| row.1 = 45.0);
    });
    ui.update();
    let anchor = handle.row_bounds(21).ok_or("anchor remains mounted")?;
    assert_eq!(anchor.origin.y, -5.0);
    assert_eq!(handle.row_top(21) - handle.row_top(20), 45.0);
    ui.press("enter");
    assert_eq!(pressed.get(), Some((22, 20)));
    batch(|| {
        model.remove(&Id(20));
        model.remove(&Id(21));
        model.move_to(&Id(23), 24);
    });
    ui.update();
    let successor = handle.row_bounds(24).ok_or("successor is mounted")?;
    assert_eq!(successor.origin.y, -5.0);
    assert!(ui.has_focus());
    Ok(())
}

#[test]
fn prepending_to_a_measured_keyed_list_reads_keys_near_the_viewport_only() {
    let [small, large] = [1_000, 100_000].map(|count| {
        let _rt = Runtime::new().enter();
        let KeyedRows {
            mut ui,
            model,
            handle,
            ..
        } = keyed_rows(0..count)
            .height(|id| if id % 3 == 0 { 35.0 } else { 20.0 })
            .build();
        handle.scroll_to_row(count / 2, Align::Top);
        ui.update();
        let before = KEY_READS.get();
        for id in count..count + 8 {
            model.prepend((Id(id), 20.0));
            ui.update();
        }
        let top = handle.top_row().get();
        assert_eq!(
            model
                .order()
                .with_untracked(|order| order.get(top).cloned()),
            Some(Id(count / 2))
        );
        KEY_READS.get() - before
    });
    assert!(
        small > 0 && large < 2 * small,
        "{small} and {large} key reads for eight prepends"
    );
}

#[test]
fn reinserting_a_key_in_a_batch_mounts_the_new_entry() -> TestResult {
    let _rt = Runtime::new().enter();
    let KeyedRows {
        mut ui,
        model,
        handle,
        mounts,
        ..
    } = keyed_rows(1..3)
        .height(|id| if id == 1 { 40.0 } else { 20.0 })
        .build();
    assert_eq!(handle.row_top(1), 40.0);
    let before = mounts.get();
    batch(|| {
        model.remove(&Id(1));
        model.prepend((Id(1), 30.0));
    });
    ui.update();
    assert_eq!(mounts.get() - before, 1);
    let replacement = handle.row_bounds(0).ok_or("replacement mounted")?;
    assert_eq!(replacement.size.height, 30.0);
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
    let (revision, font) = (signal(0), signal(16.0));
    let (mut ui, handle) = TestUiBuilder::default().size(100.0, 100.0).build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(
                || 200,
                20.0,
                move |index| {
                    if index == 0 {
                        text("This paragraph wraps to several lines at a narrow width")
                            .text_size(font)
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
            handle,
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
fn scrolling_swaps_rows_in_and_out_in_the_same_frame() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().size(200.0, 130.0).build(|| {
        virtual_list(
            || 1000,
            20.0,
            |index| text(format!("row {index}")).into_any(),
        )
        .size_full()
    });
    ui.pointer_moved(Point::new(100.0, 60.0));
    ui.scroll(Point::new(0.0, -400.0));
    ui.advance(0.0);
    let mut swaps = 0;
    for _ in 0..80 {
        let before = ui.stats();
        ui.scroll(Point::new(0.0, -20.0));
        ui.advance(0.016);
        let frame = ui.stats() - before;
        assert_eq!(
            frame.elements_created > 0,
            frame.elements_removed > 0,
            "rows mount and unmount together: {frame:?}"
        );
        swaps += usize::from(frame.elements_created > 0);
    }
    assert!(swaps > 0 && swaps < 40, "{swaps} frames swapped rows");
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

const FRAME: f64 = 1.0 / 60.0;

#[test]
fn a_fling_keeps_its_momentum_when_rows_measured_above_it_reanchor_the_list() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, handle) = TestUiBuilder::default().build_with(|| {
        let handle = virtual_list_handle();
        (
            virtual_list(|| 400, 20.0, |_| div().h(50.0))
                .measure_rows(|| 0)
                .handle(handle)
                .w(100.0)
                .h(100.0),
            handle,
        )
    });
    handle.scroll_to_row(200, Align::Top);
    ui.run(0.1);
    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll_drag(Point::new(0.0, 10.0));
    ui.advance(0.05);
    ui.scroll_drag(Point::new(0.0, 50.0));
    ui.scroll_release();
    ui.run(0.25);
    let coasting = handle.top_row().get_untracked();
    assert!(coasting < 199, "the flick moved up the list: {coasting}");
    ui.run(4.0);
    let rest = handle.top_row().get_untracked();
    assert!(
        rest + 4 <= coasting,
        "1000px/s coasts 500px in all, past the rows measured on the way: {coasting} then {rest}"
    );
    assert_eq!(ui.next_frame(), Wake::Idle);
    Ok(())
}

#[test]
fn a_fling_keeps_a_virtual_list_mounting_rows_until_it_settles_without_more_input() {
    let _rt = Runtime::new().enter();
    let (mut ui, mounted, _) = rows(100_000);
    ui.pointer_moved(Point::new(50.0, 50.0));
    flick(&mut ui, 2000.0, 0.05);
    let mut frames = 0;
    while ui.next_frame() == Wake::Now && frames < 600 {
        ui.advance(FRAME);
        frames += 1;
    }
    let first = mounted.borrow().first().copied().unwrap_or_default();
    assert!(
        first >= 30,
        "rows were swapped in as it coasted, not only when input arrived: {first}"
    );
    assert!(frames < 600, "and the UI went idle once it settled");
}

fn flick(ui: &mut TestUi, speed: f32, over: f64) {
    ui.scroll_drag(Point::new(0.0, -10.0));
    ui.advance(over);
    ui.scroll_drag(Point::new(0.0, -speed * over as f32));
    ui.scroll_release();
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

thread_local! {
    static KEY_READS: Cell<usize> = const { Cell::new(0) };
}

#[derive(Debug, PartialEq, Eq)]
struct Id(usize);

impl Clone for Id {
    fn clone(&self) -> Self {
        KEY_READS.set(KEY_READS.get() + 1);
        Id(self.0)
    }
}

impl Hash for Id {
    fn hash<T: Hasher>(&self, state: &mut T) {
        KEY_READS.set(KEY_READS.get() + 1);
        self.0.hash(state);
    }
}

struct KeyedRows {
    ui: TestUi,
    model: KeyedList<Id, (Id, f32)>,
    handle: VirtualListHandle,
    mounts: Rc<Cell<u32>>,
    pressed: Signal<Option<(usize, usize)>>,
}

struct KeyedRowsBuilder {
    ids: Range<usize>,
    height: fn(usize) -> f32,
}

fn keyed_rows(ids: Range<usize>) -> KeyedRowsBuilder {
    KeyedRowsBuilder {
        ids,
        height: |_| 20.0,
    }
}

impl KeyedRowsBuilder {
    fn height(mut self, height: fn(usize) -> f32) -> Self {
        self.height = height;
        self
    }

    fn build(self) -> KeyedRows {
        let model = keyed_list(|(id, _): &(Id, f32)| id.clone());
        model.extend(self.ids.map(|id| (Id(id), (self.height)(id))));
        let handle = virtual_list_handle();
        let (mounts, mount) = counter();
        let pressed = signal(None);
        let ui = TestUiBuilder::default().size(100.0, 100.0).build(|| {
            virtual_list(model, 20.0, move |index| {
                mount();
                let key = model
                    .order()
                    .with_untracked(|order| order.get(index).cloned());
                let Some(Id(id)) = key else {
                    return div();
                };
                let position = virtual_row_index();
                let height = model.select(Id(id), |row| row.1);
                div()
                    .test_id(format!("id-{id}"))
                    .focusable()
                    .h(move || height.get())
                    .on_key_down(move |_| pressed.set(Some((id, position.get()))))
            })
            .measure_rows(|| 0)
            .handle(handle)
            .size_full()
        });
        KeyedRows {
            ui,
            model,
            handle,
            mounts,
            pressed,
        }
    }
}
