use rouge_core::Corners;
use rouge_core::Wake;
use rouge_core::testing::{
    BLUE, CLEAR, GREEN, RED, TestResult, TestUi, TestUiBuilder, assert_near, counter,
};
use rouge_core::{
    Color, DragEvent, DragPhase, Edges, Keyframes, Menu, MenuItem, Modifiers, Point, PointerButton,
    Rect, ScrollState, Scrollable, Size, Styled, TouchPhase, Ui, UiPalette, canvas, div, dynamic,
    modifiers, pct, safe_area, scroll, show, text, window_size, zoom,
};
use rouge_reactive::{Runtime, Signal, effect, resource, signal, sleep, spawn, spawn_blocking};
use std::time::{Duration, Instant};

#[test]
fn a_hovered_view_replaced_while_patches_apply_gets_no_events_after_it_is_disposed() -> TestResult {
    let _rt = Runtime::new().enter();
    let (shown, version) = (signal(false), signal(0u32));
    let mut ui = TestUi::new(move || {
        let row = dynamic(move || {
            let hovered = signal(false);
            div()
                .test_id(format!("row-{}", version.get()))
                .w_full()
                .h(50.0)
                .on_hover(move |inside| hovered.set(inside))
        });
        let replacer = show(
            move || shown.get(),
            move || {
                dynamic(move || {
                    version.set(1);
                    div()
                })
            },
        );
        div().size_full().flex().col().child(row).child(replacer)
    });
    let row = ui.center_of("row-0")?;
    ui.pointer_moved(row);

    shown.set(true);
    ui.pointer_moved(Point::new(row.x, row.y + 100.0));

    assert!(ui.bounds_of("row-0").is_none());
    assert!(ui.bounds_of("row-1").is_some());
    Ok(())
}

#[test]
fn paint_only_change_does_not_relayout() -> TestResult {
    let _rt = Runtime::new().enter();
    let color = signal(Color::RED);
    let mut ui = TestUi::new(|| div().size_full().bg(color));
    assert_eq!(ui.passes(), (1, 1));
    assert_eq!(ui.next_frame(), Wake::Idle);
    assert!(!ui.update(), "nothing changed, nothing to do");

    color.set(Color::BLUE);
    assert_eq!(ui.next_frame(), Wake::Now);
    assert!(ui.update());
    assert_eq!(ui.passes(), (1, 2));
    assert_eq!(ui.pixel(50, 50)?, BLUE);
    Ok(())
}

#[test]
fn layout_change_relayouts_and_moves_siblings() -> TestResult {
    let _rt = Runtime::new().enter();
    let width = signal(50.0f32);
    let mut ui = TestUi::new(|| {
        div()
            .flex()
            .size_full()
            .child(div().w(width).h(10.0))
            .child(div().w(10.0).h(10.0))
    });
    assert_eq!(ui.bounds(55.0, 5.0)?.origin.x, 50.0);
    width.set(120.0);
    assert!(ui.update());
    assert_eq!(ui.stats().layouts, 2);
    assert_eq!(ui.bounds(125.0, 5.0)?, Rect::new(120.0, 0.0, 10.0, 10.0));
    Ok(())
}

#[test]
fn writes_coalesce_into_one_frame() {
    let _rt = Runtime::new().enter();
    let a = signal(Color::RED);
    let b = signal(Color::RED);
    let mut ui = TestUi::new(|| div().size_full().bg(a).child(div().size(10.0).bg(b)));

    a.set(Color::BLUE);
    b.set(Color::BLUE);
    a.set(Color::GREEN);
    assert_eq!(ui.next_frame(), Wake::Now);
    ui.update();
    assert_eq!(ui.stats().paints, 2, "one paint for all three writes");
    assert_eq!(ui.next_frame(), Wake::Idle);
    b.set(Color::RED);
    assert_eq!(ui.next_frame(), Wake::Now);
}

#[test]
fn hover_and_active_variants_restyle_and_revert() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, _) = clicker();
    let color = |ui: &mut TestUi| {
        ui.update();
        ui.render().map(|img| img.pixel(50, 25))
    };
    assert_eq!(color(&mut ui)?, RED);
    ui.pointer_moved(Point::new(50.0, 25.0));
    assert_eq!(color(&mut ui)?, GREEN, "hovered");
    ui.pointer_down(PointerButton::Primary);
    assert_eq!(color(&mut ui)?, BLUE, "pressed");
    ui.pointer_up(PointerButton::Primary);
    assert_eq!(color(&mut ui)?, GREEN, "released, still hovered");
    ui.pointer_left();
    assert_eq!(color(&mut ui)?, RED, "pointer left");
    assert_eq!(
        ui.stats().layouts,
        1,
        "variants that only repaint never relayout"
    );
    Ok(())
}

#[test]
fn hovering_elements_without_variants_is_free() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| div().size_full().child(div().size(50.0)));
    ui.pointer_moved(Point::new(10.0, 10.0));
    ui.pointer_moved(Point::new(150.0, 90.0));
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_up(PointerButton::Primary);
    assert_eq!(ui.next_frame(), Wake::Idle);
}

#[test]
fn click_requires_press_and_release_on_the_same_element() {
    let _rt = Runtime::new().enter();
    let (mut ui, clicks) = clicker();
    ui.click(Point::new(50.0, 25.0));
    assert_eq!(clicks.get_untracked(), 1);

    ui.click(Point::new(150.0, 75.0));
    assert_eq!(clicks.get_untracked(), 1, "clicked outside");

    ui.pointer_moved(Point::new(50.0, 25.0));
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_moved(Point::new(150.0, 75.0));
    ui.pointer_up(PointerButton::Primary);
    assert_eq!(clicks.get_untracked(), 1, "released elsewhere");

    ui.pointer_moved(Point::new(50.0, 25.0));
    ui.pointer_down(PointerButton::Secondary);
    ui.pointer_up(PointerButton::Secondary);
    assert_eq!(clicks.get_untracked(), 1, "secondary button does not click");
}

#[test]
fn later_siblings_are_hit_first_and_overflow_hidden_clips() -> TestResult {
    let _rt = Runtime::new().enter();
    let hit = signal("");
    let mut ui = TestUi::new(|| {
        let overflowing = div()
            .w(150.0)
            .h(20.0)
            .bg(Color::BLUE)
            .on_click(move |_| hit.set("clipped child"));
        let top = div()
            .absolute()
            .left(20.0)
            .top(30.0)
            .size(20.0)
            .bg(Color::GREEN);
        div()
            .size_full()
            .child(
                div()
                    .absolute()
                    .size(50.0)
                    .overflow_hidden()
                    .bg(Color::RED)
                    .child(overflowing),
            )
            .child(top.on_click(move |_| hit.set("top")))
            .on_click(move |_| {
                if hit.get_untracked().is_empty() {
                    hit.set("background");
                }
            })
    });
    let img = ui.render()?;
    assert_eq!(img.pixel(10, 10), BLUE, "child inside clip");
    assert_eq!(img.pixel(100, 10), CLEAR, "child clipped outside parent");
    assert_eq!(img.pixel(25, 35), GREEN, "later sibling paints on top");

    ui.click(Point::new(100.0, 10.0));
    assert_eq!(
        hit.get_untracked(),
        "background",
        "clipped area is not hittable"
    );
    hit.set("");
    ui.click(Point::new(10.0, 10.0));
    assert_eq!(hit.get_untracked(), "clipped child");
    hit.set("");
    ui.click(Point::new(25.0, 35.0));
    assert_eq!(hit.get_untracked(), "top");
    Ok(())
}

#[test]
fn dropping_the_ui_disposes_its_scope() {
    let _rt = Runtime::new().enter();
    let (ui, color) = TestUiBuilder::default().build_with(|| {
        let color = signal(Color::RED);
        (div().bg(color), color)
    });
    assert!(color.is_alive());
    drop(ui);
    assert!(!color.is_alive());
}

#[test]
fn resize_relayouts() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| div().size_full().child(div().w(pct(50.0)).h(10.0)));
    assert_eq!(ui.bounds(5.0, 5.0)?.size.width, 100.0);
    ui.set_size(Size::new(400.0, 100.0));
    assert!(ui.update());
    assert_eq!(ui.bounds(5.0, 5.0)?.size.width, 200.0);
    Ok(())
}

#[test]
fn a_window_that_shrinks_and_grows_back_matches_one_drawn_fresh_at_that_size() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(footer);
    ui.render()?;
    ui.set_size(Size::new(200.0, 80.0));
    ui.render()?;
    ui.set_size(Size::new(200.0, 100.0));
    let grown = ui.render()?;

    let fresh = TestUi::canvas(footer).render()?;
    assert_eq!(grown.pixels, fresh.pixels);
    Ok(())
}

#[test]
fn clicks_and_scrolls_bubble_from_the_element_under_the_pointer_and_can_be_stopped() {
    let _rt = Runtime::new().enter();
    let log = signal(Vec::<(&'static str, f32)>::new());
    let mut ui = TestUi::new(|| {
        let layer = move |name: &'static str, stop: bool| {
            div()
                .on_click(move |e| {
                    log.update(|l| l.push((name, 0.0)));
                    if stop {
                        e.stop_propagation();
                    }
                })
                .on_scroll(move |e| {
                    log.update(|l| l.push((name, e.delta.y)));
                    if stop {
                        e.stop_propagation();
                    }
                })
        };
        let inner = layer("inner", false).w(100.0).h(50.0);
        layer("outer", false)
            .size_full()
            .child(inner.child(layer("stopper", true).size(20.0)))
    });
    ui.scroll(Point::new(0.0, -8.0));
    assert!(
        log.get_untracked().is_empty(),
        "no pointer, nothing to scroll"
    );

    ui.click(Point::new(50.0, 40.0));
    ui.scroll(Point::new(0.0, -8.0));
    assert_eq!(
        log.get_untracked(),
        [
            ("inner", 0.0),
            ("outer", 0.0),
            ("inner", -8.0),
            ("outer", -8.0)
        ]
    );

    log.set(Vec::new());
    ui.click(Point::new(5.0, 5.0));
    ui.scroll(Point::new(0.0, 4.0));
    assert_eq!(log.get_untracked(), [("stopper", 0.0), ("stopper", 4.0)]);
}

#[test]
fn scrolling_content_under_a_still_pointer_updates_hover() -> TestResult {
    let _rt = Runtime::new().enter();
    let offset = signal(0.0f32);
    let mut ui = TestUi::new(|| {
        let row = |color| div().h(50.0).bg(color).hover(|s| s.bg(Color::GREEN));
        div()
            .size_full()
            .on_scroll(move |e| offset.update(|o| *o += e.delta.y))
            .child(
                div()
                    .translate_y(offset)
                    .child(row(Color::RED))
                    .child(row(Color::BLUE)),
            )
    });
    ui.pointer_moved(Point::new(10.0, 25.0));
    ui.update();
    assert_eq!(ui.pixel(10, 25)?, GREEN, "first row hovered");

    ui.scroll(Point::new(0.0, -50.0));
    ui.update();
    let img = ui.render()?;
    assert_eq!(
        img.pixel(10, 25),
        GREEN,
        "second row scrolled under the pointer"
    );
    assert_eq!(ui.stats().layouts, 1, "scrolling a layer never relayouts");
    Ok(())
}

#[test]
fn window_size_tracks_resizes() {
    let _rt = Runtime::new().enter();
    let seen = signal(Size::ZERO);
    let mut ui = TestUi::new(move || {
        let size = window_size();
        effect(move || seen.set(size.get()));
        div()
    });
    assert_eq!(seen.get_untracked(), Size::new(200.0, 100.0));
    ui.set_size(Size::new(400.0, 300.0));
    assert_eq!(seen.get_untracked(), Size::new(400.0, 300.0));
}

#[test]
fn safe_area_starts_empty_and_tracks_the_insets_in_zoomed_points() {
    let _rt = Runtime::new().enter();
    let seen = signal(Edges::all(-1.0));
    let (mut ui, zoom) = TestUiBuilder::default().build_with(move || {
        let insets = safe_area();
        effect(move || seen.set(insets.get()));
        (div(), zoom())
    });
    assert_eq!(seen.get_untracked(), Edges::ZERO);
    let notch = Edges {
        top: 48.0,
        right: 0.0,
        bottom: 34.0,
        left: 0.0,
    };
    ui.set_safe_area(notch);
    assert_eq!(seen.get_untracked(), notch);
    zoom.set(2.0);
    ui.advance(0.0);
    assert_eq!(
        seen.get_untracked(),
        Edges {
            top: 24.0,
            right: 0.0,
            bottom: 17.0,
            left: 0.0,
        }
    );
}

#[test]
fn modifiers_track_the_held_keys() {
    let _rt = Runtime::new().enter();
    let seen = signal(Modifiers::META);
    let mut ui = TestUi::new(move || {
        let held = modifiers();
        effect(move || seen.set(held.get()));
        div()
    });
    assert_eq!(seen.get_untracked(), Modifiers::NONE);
    ui.set_modifiers(Modifiers::COMMAND);
    assert_eq!(seen.get_untracked(), Modifiers::COMMAND);
    ui.set_modifiers(Modifiers::NONE);
    assert_eq!(seen.get_untracked(), Modifiers::NONE);
}

#[test]
fn zoom_shrinks_the_logical_window_and_paints_everything_at_the_higher_scale() -> TestResult {
    let _rt = Runtime::new().enter();
    let seen = signal(Size::ZERO);
    let (mut ui, zoom) = TestUiBuilder::default().build_with(move || {
        let size = window_size();
        effect(move || seen.set(size.get()));
        let block = div()
            .test_id("block")
            .w(32.0)
            .h(20.0)
            .rounded(Corners::all(8.0))
            .bg(Color::RED);
        (div().test_id("root").size_full().child(block), zoom())
    });
    assert_near(ui.pixel(3, 3)?, RED, "inside the corner radius");
    assert_eq!(ui.pixel(40, 15)?, CLEAR);
    zoom.set(2.0);
    ui.advance(0.0);
    assert_eq!(ui.scale(), 2.0);
    assert_eq!(seen.get_untracked(), Size::new(100.0, 50.0));
    assert_eq!(ui.rect_of("root")?, Rect::new(0.0, 0.0, 100.0, 50.0));
    assert_eq!(ui.rect_of("block")?, Rect::new(0.0, 0.0, 32.0, 20.0));
    assert_eq!(ui.pixel(40, 15)?, RED);
    assert_eq!(ui.pixel(70, 10)?, CLEAR);
    assert_near(ui.pixel(3, 3)?, CLEAR, "the corner radius doubled too");
    zoom.set(1.0);
    ui.advance(0.0);
    assert_eq!(seen.get_untracked(), Size::new(200.0, 100.0));
    assert_eq!(ui.pixel(40, 15)?, CLEAR);
    Ok(())
}

#[test]
fn physical_pointer_positions_divided_by_the_scale_hit_what_is_painted_there() -> TestResult {
    let _rt = Runtime::new().enter();
    let (left, right) = (signal(false), signal(false));
    let (mut ui, zoom) = TestUiBuilder::default().scale(2.0).build_with(move || {
        let half = |color, hovered: Signal<bool>| {
            div()
                .w(50.0)
                .h(20.0)
                .bg(color)
                .on_hover(move |inside| hovered.set(inside))
        };
        let row = div()
            .flex()
            .row()
            .child(half(Color::RED, left))
            .child(half(Color::BLUE, right));
        (row, zoom())
    });
    zoom.set(1.5);
    ui.advance(0.0);
    assert_eq!(ui.scale(), 3.0);
    assert_eq!(ui.pixel(180, 30)?, BLUE);
    let scale = ui.scale();
    ui.pointer_moved(Point::new(180.0 / scale, 30.0 / scale));
    ui.advance(0.0);
    assert!(right.get_untracked() && !left.get_untracked());
    zoom.set(3.0);
    ui.advance(0.0);
    assert_eq!(ui.pixel(180, 30)?, RED);
    assert!(
        left.get_untracked() && !right.get_untracked(),
        "a still pointer keeps its physical position across zoom changes"
    );
    Ok(())
}

#[test]
fn a_captured_pointer_keeps_delivering_moves_and_the_release_even_outside_the_window() -> TestResult
{
    let _rt = Runtime::new().enter();
    let log = signal(Vec::<(&'static str, f32)>::new());
    let mut ui = TestUi::new(|| {
        let note = move |what: &'static str, x: f32| log.update(|log| log.push((what, x)));
        let handle = div()
            .size(20.0)
            .bg(Color::RED)
            .hover(|s| s.bg(Color::GREEN))
            .on_pointer_move(move |e| note("move", e.position.x))
            .on_pointer_up(move |e| note("up", e.position.x))
            .on_pointer_down(move |e| {
                e.capture();
                note("down", e.position.x);
            });
        div().size_full().child(handle)
    });
    ui.pointer_moved(Point::new(10.0, 10.0));
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_moved(Point::new(150.0, 50.0));
    ui.update();
    assert_eq!(
        ui.pixel(10, 10)?,
        GREEN,
        "still hovered while it holds the pointer"
    );
    ui.pointer_up(PointerButton::Primary);
    assert_eq!(
        log.get_untracked(),
        [
            ("move", 10.0),
            ("down", 10.0),
            ("move", 150.0),
            ("up", 150.0)
        ]
    );

    ui.pointer_moved(Point::new(160.0, 50.0));
    ui.update();
    assert_eq!(log.get_untracked().len(), 4, "released with the button");
    assert_eq!(ui.pixel(10, 10)?, RED);

    ui.pointer_moved(Point::new(10.0, 10.0));
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_left();
    ui.update();
    assert_eq!(
        ui.pixel(10, 10)?,
        GREEN,
        "leaving mid-press changes nothing yet"
    );
    ui.pointer_up(PointerButton::Primary);
    ui.update();
    let log = log.get_untracked();
    assert_eq!(
        (log.len(), log[6].0),
        (7, "up"),
        "the release still arrives"
    );
    assert_eq!(ui.pixel(10, 10)?, RED, "and then the pointer is gone");
    Ok(())
}

#[test]
fn pointer_moves_reach_only_the_elements_under_the_pointer() {
    let _rt = Runtime::new().enter();
    let (inner_moves, inner) = counter();
    let (outer_moves, outer) = counter();
    let mut ui = TestUi::new(|| {
        let child = div().size(20.0).on_pointer_move(move |_| inner());
        div()
            .size_full()
            .on_pointer_move(move |_| outer())
            .child(child)
    });
    ui.pointer_moved(Point::new(10.0, 10.0));
    ui.pointer_moved(Point::new(100.0, 50.0));
    assert_eq!((inner_moves.get(), outer_moves.get()), (1, 2));
}

#[test]
fn drags_start_past_the_slop_report_velocity_and_replace_the_click() -> TestResult {
    let _rt = Runtime::new().enter();
    let events = signal(Vec::<DragEvent>::new());
    let (clicks, click) = counter();
    let mut ui = TestUi::new(|| {
        let handle = div()
            .size(40.0)
            .on_drag(move |drag| events.update(|events| events.push(*drag)))
            .on_click(move |_| click());
        div().size_full().child(handle)
    });
    ui.pointer_moved(Point::new(10.0, 10.0));
    ui.pointer_down(PointerButton::Primary);
    for x in [12.0, 30.0, 50.0, 70.0] {
        ui.advance(0.01);
        ui.pointer_moved(Point::new(x, 10.0));
        if x == 12.0 {
            assert!(
                events.get_untracked().is_empty(),
                "a couple of pixels is not a drag yet"
            );
        }
    }
    ui.pointer_up(PointerButton::Primary);

    let events = events.get_untracked();
    let phases: Vec<_> = events.iter().map(|drag| drag.phase).collect();
    let moving = [DragPhase::Move; 3];
    assert_eq!(
        phases,
        [&[DragPhase::Start][..], &moving, &[DragPhase::End]].concat()
    );
    assert_eq!(
        events[0].position,
        Point::new(10.0, 10.0),
        "it starts where the pointer went down"
    );
    assert_eq!(events[1].translation, Point::new(20.0, 0.0));
    assert_eq!(events[2].delta, Point::new(20.0, 0.0));
    assert_eq!(
        events[3].position.x, 70.0,
        "the pointer left the element but the drag kept it"
    );
    let end = events.last().ok_or("an end event")?;
    assert!(
        (end.velocity.x - 1500.0).abs() < 1.0,
        "60px over 40ms: {:?}",
        end.velocity
    );
    assert_eq!(end.translation, Point::new(60.0, 0.0));
    assert_eq!(clicks.get(), 0, "a drag is not a click");

    ui.click(Point::new(10.0, 10.0));
    assert_eq!(clicks.get(), 1, "a press that does not move still is");
    Ok(())
}

#[test]
fn pointer_events_and_tracked_bounds_see_through_padding_scroll_and_transforms() {
    let _rt = Runtime::new().enter();
    let bounds = signal(Rect::ZERO);
    let seen = signal(None);
    let mut ui = TestUi::new(move || {
        let target = div()
            .size(40.0)
            .translate_x(20.0)
            .track_bounds(bounds)
            .on_pointer_down(move |event| {
                seen.set(Some((
                    event.local(),
                    event.modifiers.shift,
                    event.click_count,
                )));
            });
        div().size_full().p(10.0).child(
            scroll()
                .size(80.0)
                .child(div().h(300.0).pt(30.0).child(target)),
        )
    });
    ui.advance(0.0);
    assert_eq!(bounds.get(), Rect::new(30.0, 40.0, 40.0, 40.0));
    ui.pointer_moved(Point::new(50.0, 50.0));
    ui.scroll(Point::new(0.0, -20.0));
    ui.advance(0.0);
    assert_eq!(
        bounds.get(),
        Rect::new(30.0, 20.0, 40.0, 40.0),
        "scrolled without layout"
    );
    ui.set_modifiers(Modifiers::SHIFT);
    ui.click(Point::new(35.0, 25.0));
    ui.click(Point::new(35.0, 25.0));
    assert_eq!(
        seen.get_untracked(),
        Some((Point::new(5.0, 5.0), true, 2)),
        "through padding, scroll and transform"
    );
    ui.update_at(5.0);
    ui.click(Point::new(35.0, 25.0));
    assert_eq!(
        seen.get_untracked().map(|(_, _, count)| count),
        Some(1),
        "a slow second click starts over"
    );
}

#[test]
fn pass_through_elements_let_pointer_events_reach_what_is_behind_them() {
    let _rt = Runtime::new().enter();
    let (behind, hit_behind) = counter();
    let (child, hit_child) = counter();
    let mut ui = TestUi::new(move || {
        let panel = div().absolute().inset(0.0).pass_through();
        div()
            .size_full()
            .child(div().size_full().on_click(move |_| hit_behind()))
            .child(panel.child(div().size(20.0).on_click(move |_| hit_child())))
    });
    ui.click(Point::new(100.0, 50.0));
    ui.click(Point::new(10.0, 10.0));
    assert_eq!(
        (behind.get(), child.get()),
        (1, 1),
        "the panel is skipped but its children are not"
    );
}

#[test]
fn spawned_tasks_and_resources_update_the_ui_when_polled() -> TestResult {
    let _rt = Runtime::new().enter();
    let id = signal(1u32);
    let mut ui = TestUi::new(move || {
        let user = resource(
            move || id.get(),
            |id| async move { spawn_blocking(move || id * 10).await.unwrap_or(0) },
        );
        let width = signal(10.0f32);
        spawn(async move {
            sleep(Duration::from_millis(1)).await;
            width.set(60.0);
        });
        div()
            .size_full()
            .child(div().h(10.0).w(move || user.get().unwrap_or(0) as f32))
            .child(div().h(10.0).w(width))
    });
    let widths = |ui: &mut TestUi| {
        (
            ui.bounds(1.0, 5.0).ok().map(|b| b.size.width),
            ui.bounds(1.0, 15.0).ok().map(|b| b.size.width),
        )
    };
    assert_eq!(widths(&mut ui).1, Some(10.0), "nothing has resolved yet");
    ui.wait_until(|ui| widths(ui) == (Some(10.0), Some(60.0)));
    assert_eq!(
        widths(&mut ui),
        (Some(10.0), Some(60.0)),
        "the resource and the task both finished"
    );
    id.set(7);
    ui.wait_until(|ui| widths(ui).0 == Some(70.0));
    assert_eq!(widths(&mut ui).0, Some(70.0), "the resource refetched");
    Ok(())
}

#[test]
fn text_of_reads_the_current_text_inside_a_test_id() {
    let _rt = Runtime::new().enter();
    let (mut ui, name) = TestUiBuilder::default().build_with(|| {
        let name = signal("Ada".to_owned());
        let card = div()
            .test_id("card")
            .child(text("Hello, "))
            .child(div().child(text(move || name.get())));
        (div().child(card).child(text("outside")), name)
    });
    assert_eq!(ui.text_of("card").as_deref(), Some("Hello, Ada"));
    name.set("Grace".to_owned());
    ui.flush();
    assert_eq!(ui.text_of("card").as_deref(), Some("Hello, Grace"));
    assert_eq!(ui.text_of("missing"), None);
}

#[test]
fn menu_shortcuts_dispatch_when_no_element_binding_handles_the_chord() -> TestResult {
    let _rt = Runtime::new().enter();
    let (refreshed, quit) = (signal(0), signal(0));
    let menus = vec![Menu::new(
        "View",
        vec![
            MenuItem::action("Refresh", Refresh).shortcut("cmd-r"),
            MenuItem::callback("Quit", move || quit.update(|quit| *quit += 1)).shortcut("cmd-q"),
        ],
    )];
    let mut ui = TestUi::new(move || {
        div()
            .size_full()
            .on_action(move |_: &Refresh| refreshed.update(|count| *count += 1))
    });
    ui.bind_menus(&menus);
    assert!(ui.press("cmd-r"));
    assert!(ui.press("cmd-q"));
    assert!(!ui.press("cmd-x"));
    assert_eq!((refreshed.get(), quit.get()), (1, 1));
    Ok(())
}

#[test]
fn a_tap_clicks_shows_its_press_and_leaves_no_hover_behind() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, clicks) = clicker();
    let at = Point::new(20.0, 20.0);
    ui.touch(TouchPhase::Start, at);
    ui.update();
    assert_eq!(ui.pixel(20, 20)?, BLUE, "pressed while the finger is down");
    ui.touch(TouchPhase::Move, Point::new(22.0, 21.0));
    ui.touch(TouchPhase::End, Point::new(22.0, 21.0));
    ui.update();
    assert_eq!(clicks.get(), 1);
    assert_eq!(ui.pixel(20, 20)?, RED, "no hover once the finger lifts");
    Ok(())
}

#[test]
fn a_touch_pan_scrolls_without_clicking_and_flings_on_release() {
    let _rt = Runtime::new().enter();
    let (mut ui, state, clicks) = touch_scroller();
    ui.touch(TouchPhase::Start, Point::new(50.0, 80.0));
    for y in [70.0, 50.0, 30.0] {
        ui.advance(0.01);
        ui.touch(TouchPhase::Move, Point::new(50.0, y));
    }
    ui.advance(0.0);
    assert_eq!(
        state.get_untracked().offset.y,
        50.0,
        "content follows the finger"
    );
    ui.touch(TouchPhase::End, Point::new(50.0, 30.0));
    ui.run(0.1);
    assert!(
        state.get_untracked().offset.y > 50.0,
        "keeps moving after the finger lifts"
    );
    ui.run(5.0);
    assert_eq!(clicks.get(), 0, "a pan is not a tap");
    assert_eq!(ui.next_frame(), Wake::Idle);
}

#[test]
fn a_touch_cancel_ends_the_press_without_a_click() {
    let _rt = Runtime::new().enter();
    let (mut ui, clicks) = clicker();
    ui.touch(TouchPhase::Start, Point::new(20.0, 20.0));
    ui.touch(TouchPhase::Cancel, Point::new(20.0, 20.0));
    assert_eq!(clicks.get(), 0);
}

#[test]
fn a_touch_on_a_drag_target_drags_it_instead_of_panning() {
    let _rt = Runtime::new().enter();
    let events = signal(Vec::<DragPhase>::new());
    let state = signal(ScrollState::default());
    let mut ui = TestUi::new(move || {
        let handle = div()
            .w_full()
            .h(40.0)
            .on_drag(move |drag| events.update(|events| events.push(drag.phase)));
        let content = div().child(handle).child(div().h(400.0));
        div().size_full().child(
            scroll()
                .w(100.0)
                .h(100.0)
                .scroll_state(state)
                .child(content),
        )
    });
    ui.touch(TouchPhase::Start, Point::new(20.0, 20.0));
    ui.touch(TouchPhase::Move, Point::new(20.0, 5.0));
    ui.touch(TouchPhase::End, Point::new(20.0, 5.0));
    ui.advance(0.0);
    assert_eq!(
        events.get_untracked(),
        [DragPhase::Start, DragPhase::Move, DragPhase::End]
    );
    assert_eq!(state.get_untracked().offset.y, 0.0);
}

#[test]
fn each_pointer_listener_sees_its_own_local_position_and_size() {
    let _rt = Runtime::new().enter();
    let (outer, inner) = (signal(None), signal(None));
    let mut ui = TestUi::new(move || {
        let target = div()
            .size(40.0)
            .h(20.0)
            .translate_x(30.0)
            .on_pointer_down(move |event| inner.set(Some((event.local(), event.size()))));
        div()
            .w(100.0)
            .h(60.0)
            .p(10.0)
            .on_pointer_down(move |event| outer.set(Some((event.local(), event.size()))))
            .child(target)
    });
    ui.click(Point::new(45.0, 15.0));
    assert_eq!(
        inner.get_untracked(),
        Some((Point::new(5.0, 5.0), Size::new(40.0, 20.0))),
        "relative to the translated target"
    );
    assert_eq!(
        outer.get_untracked(),
        Some((Point::new(45.0, 15.0), Size::new(100.0, 60.0))),
        "relative to the ancestor it bubbled to"
    );
}

#[test]
fn writes_wait_in_the_queue_until_flushed_and_lay_out_on_the_next_frame() {
    let _rt = Runtime::new().enter();
    let (width, label) = (signal(10.0_f32), signal("before".to_owned()));
    let mut ui = TestUi::new(move || {
        div()
            .test_id("box")
            .w(move || width.get())
            .h(10.0)
            .child(text(move || label.get()))
    });
    let laid_out = |ui: &TestUi| ui.bounds_of("box").map(|bounds| bounds.size.width);
    width.set(20.0);
    label.set("after".to_owned());
    assert_eq!(ui.next_frame(), Wake::Now);
    assert_eq!(laid_out(&ui), Some(10.0), "queries read the last frame");
    assert_eq!(ui.text_of("box").as_deref(), Some("before"));
    ui.flush();
    assert_eq!(ui.text_of("box").as_deref(), Some("after"), "flushed");
    assert_eq!(laid_out(&ui), Some(10.0), "a flush does not lay out");
    assert_eq!(ui.next_frame(), Wake::Now);
    assert!(ui.update());
    assert_eq!(laid_out(&ui), Some(20.0));
    assert_eq!(ui.next_frame(), Wake::Idle);
}

#[test]
fn geometry_observers_feed_back_once_per_frame_and_the_rest_waits_for_the_next() {
    let _rt = Runtime::new().enter();
    let (first, second) = (signal(Size::ZERO), signal(Size::ZERO));
    let mut ui = TestUi::new(move || {
        let half = |size: Signal<Size>| move || size.get().width / 2.0;
        div()
            .flex()
            .child(div().w(40.0).h(10.0).track_size(first))
            .child(div().test_id("b").w(half(first)).h(10.0).track_size(second))
            .child(div().test_id("c").w(half(second)).h(10.0))
    });
    let width = |ui: &TestUi, name| ui.bounds_of(name).map(|bounds| bounds.size.width);
    assert_eq!(width(&ui, "b"), Some(20.0), "re-laid out in the same frame");
    assert_eq!(width(&ui, "c"), Some(0.0), "a second feedback waits");
    assert_eq!(ui.stats().relayouts, 1);
    assert_eq!(ui.next_frame(), Wake::Now, "and asks for the next frame");
    ui.update();
    assert_eq!(width(&ui, "c"), Some(10.0));
    assert_eq!(ui.stats().relayouts, 1);
    assert_eq!(ui.next_frame(), Wake::Idle);
}

#[test]
fn a_canvas_resized_by_measurement_feedback_draws_at_its_settled_size_in_the_same_frame() {
    let _rt = Runtime::new().enter();
    let measured = signal(Size::ZERO);
    let ui = TestUi::new(move || {
        let drawn = canvas(|painter| {
            let size = painter.size();
            painter.region("drawn", Rect::new(0.0, 0.0, size.width, size.height));
        });
        div()
            .flex()
            .child(div().w(40.0).h(10.0).track_size(measured))
            .child(drawn.h(10.0).w(move || measured.get().width / 2.0))
    });
    let drawn = ui.bounds_of("drawn").map(|bounds| bounds.size.width);
    assert_eq!(drawn, Some(20.0));
    assert_eq!(ui.stats().relayouts, 1);
}

fn clicker() -> (TestUi, Signal<u32>) {
    let clicks = signal(0u32);
    let ui = TestUi::new(|| {
        let target = div()
            .w(100.0)
            .h(50.0)
            .bg(Color::RED)
            .hover(|s| s.bg(Color::GREEN))
            .active(|s| s.bg(Color::BLUE))
            .on_click(move |_| clicks.update(|c| *c += 1));
        div().size_full().bg(Color::WHITE).child(target)
    });
    (ui, clicks)
}

#[derive(Clone, Copy)]
struct Refresh;

fn touch_scroller() -> (TestUi, Signal<ScrollState>, Signal<u32>) {
    let state = signal(ScrollState::default());
    let clicks = signal(0u32);
    let ui = TestUi::new(move || {
        let content = div()
            .h(1000.0)
            .on_click(move |_| clicks.update(|c| *c += 1));
        div().size_full().child(
            scroll()
                .w(100.0)
                .h(100.0)
                .scroll_state(state)
                .child(content),
        )
    });
    (ui, state, clicks)
}

#[test]
fn the_clear_color_is_the_root_background_or_the_theme_canvas() {
    let _rt = Runtime::new().enter();
    let ui = Ui::new(Size::new(100.0, 100.0), || div().size_full().bg(Color::RED));
    assert_eq!(ui.clear_color(), Color::RED);
    let ui = Ui::new(Size::new(100.0, 100.0), || div().size_full());
    assert_eq!(
        ui.clear_color(),
        Color::from(UiPalette::light().canvas),
        "an unpainted root shows the theme's canvas"
    );
}

#[test]
fn a_frame_without_observers_lays_out_exactly_once() {
    let _rt = Runtime::new().enter();
    let width = signal(10.0);
    let mut ui = TestUi::new(move || div().flex().child(div().w(move || width.get()).h(10.0)));
    let before = ui.stats();
    width.set(30.0);
    ui.advance(0.0);
    let work = ui.stats() - before;
    assert_eq!((work.layouts, work.relayouts), (1, 0));
}

#[test]
fn a_size_observer_sees_the_new_size_in_the_same_frame_with_one_relayout() -> TestResult {
    let _rt = Runtime::new().enter();
    let width = signal(40.0);
    let measured = signal(Size::ZERO);
    let mut ui = TestUi::new(move || {
        div()
            .flex()
            .child(div().w(move || width.get()).h(10.0).track_size(measured))
            .child(
                div()
                    .test_id("follower")
                    .h(10.0)
                    .w(move || measured.get().width / 2.0),
            )
    });
    let before = ui.stats();
    width.set(80.0);
    ui.advance(0.0);
    let work = ui.stats() - before;
    assert_eq!(measured.get_untracked().width, 80.0);
    assert_eq!(ui.bounds_of("follower").ok_or("follower")?.size.width, 40.0);
    assert_eq!(
        (work.layouts, work.relayouts),
        (2, 1),
        "feedback is bounded to one relayout"
    );
    Ok(())
}

#[test]
fn a_commit_carries_the_scale_its_content_was_painted_at_whenever_it_changes() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, zoom) = TestUiBuilder::default()
        .scale(2.0)
        .build_with(|| (div().size_full().bg(Color::RED), zoom()));
    assert_eq!(ui.commit().scale, Some(2.0));
    ui.advance(0.0);
    assert_eq!(ui.commit().scale, None, "an unchanged scale is not resent");
    zoom.set(0.75);
    ui.advance(0.0);
    assert_eq!(ui.commit().scale, Some(1.5), "zoom reaches the compositor");
    Ok(())
}

#[test]
fn the_clock_counts_from_the_epoch_the_host_shares_with_the_compositor() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = Ui::new(Size::new(10.0, 10.0), div);
    let epoch = Instant::now()
        .checked_sub(Duration::from_secs(5))
        .ok_or("the epoch underflowed")?;
    ui.set_epoch(epoch);
    assert!(ui.time() >= 5.0, "time is measured from the shared epoch");
    Ok(())
}

#[test]
fn tracked_bounds_riding_on_a_looping_slide_keep_the_ui_following_only_while_mounted() {
    let _rt = Runtime::new().enter();
    let (shown, bounds) = (signal(true), signal(Rect::ZERO));
    let mut ui = TestUi::new(move || {
        div().size(40.0).animate(slide()).child(show(
            move || shown.get(),
            move || div().size(10.0).track_bounds(bounds),
        ))
    });
    ui.update_at(0.0);
    assert_eq!(ui.next_frame(), Wake::Now, "the bounds move with the slide");
    ui.update_at(0.5);
    assert_eq!(
        bounds.get_untracked().origin.x,
        20.0,
        "and are published as it runs"
    );

    shown.set(false);
    ui.update_at(0.6);
    assert_eq!(ui.next_frame(), Wake::Idle, "nothing watches the slide");

    shown.set(true);
    ui.update_at(0.7);
    assert_eq!(
        ui.next_frame(),
        Wake::Now,
        "a newly mounted observer is followed"
    );
}

#[test]
fn a_size_observer_does_not_follow_a_moving_parent() {
    let _rt = Runtime::new().enter();
    let measured = signal(Size::ZERO);
    let mut ui = TestUi::new(move || {
        div()
            .size(40.0)
            .animate(slide())
            .child(div().size(10.0).track_size(measured))
    });
    ui.update_at(0.0);
    assert_eq!(measured.get_untracked(), Size::new(10.0, 10.0));
    assert_eq!(
        ui.next_frame(),
        Wake::Idle,
        "a transform cannot change a size"
    );
}

fn slide() -> Keyframes {
    Keyframes::new()
        .at(0.0, |pose| pose.translate_x(0.0))
        .at(1.0, |pose| pose.translate_x(40.0))
        .repeat()
}

fn footer() -> impl rouge_core::View {
    div()
        .size_full()
        .flex()
        .col()
        .child(div().flex_1().bg(Color::BLUE))
        .child(div().h(10.0).bg(Color::RED))
}
