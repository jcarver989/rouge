use rouge_core::testing::{BLACK, BLUE, GREEN, RED, TestResult, TestUi, WHITE, counter};
use rouge_core::{
    Color, Point, ScrollEvent, ScrollState, Scrollable, Size, Styled, View, div, scroll,
    virtual_list,
};
use rouge_reactive::{Runtime, Signal, effect, on_cleanup, signal};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

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
    assert_eq!(ui.stats(), stats, "no layout, no paint, no layer repainted");

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
    assert_eq!(ui.stats(), stats, "no layout and no paint");
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
