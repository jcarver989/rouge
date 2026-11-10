use rouge_core::testing::{BLUE, CLEAR, GREEN, RED, TestResult, TestUi, TestUiBuilder, counter};
use rouge_core::{
    Color, DragEvent, DragPhase, Modifiers, Point, PointerButton, Rect, Size, Styled, div, pct,
    scroll, window_size,
};
use rouge_reactive::{Runtime, Signal, background, effect, resource, signal, sleep, spawn};
use std::time::Duration;

#[test]
fn paint_only_change_does_not_relayout() -> TestResult {
    let _rt = Runtime::new().enter();
    let color = signal(Color::RED);
    let mut ui = TestUi::new(|| div().size_full().bg(color));
    assert_eq!(ui.passes(), (1, 1));
    assert!(!ui.needs_frame());
    assert!(!ui.update(), "nothing changed, nothing to do");

    color.set(Color::BLUE);
    assert!(ui.needs_frame());
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
    assert!(ui.needs_frame());
    ui.update();
    assert_eq!(ui.stats().paints, 2, "one paint for all three writes");
    assert!(!ui.needs_frame());
    b.set(Color::RED);
    assert!(ui.needs_frame());
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
    assert!(!ui.needs_frame());
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
            |id| async move { background(async move { id * 10 }).await },
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
