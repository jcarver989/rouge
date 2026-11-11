mod common;

use common::builder;
use rouge_core::testing::{TestResult, TestUi, counter};
use rouge_core::{Key, Point, Rect, Styled, div, show};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{Placement, button, popover, tooltip};
use std::thread::sleep;
use std::time::Duration;

#[test]
fn popover_opens_below_its_trigger_and_closes_on_outside_click_or_escape() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(false);
    let (inside, hit_inside) = counter();
    let (behind, hit_behind) = counter();
    let mut ui = builder().build(move || {
        let trigger = button("Open")
            .w(60.0)
            .h(24.0)
            .on_click(move || open.update(|open| *open = !*open));
        let content = move || {
            let hit_inside = hit_inside.clone();
            div().size(80.0).on_click(move |_| hit_inside())
        };
        div()
            .size_full()
            .p(10.0)
            .child(popover(open, trigger, content))
            .child(
                div()
                    .absolute()
                    .left(200.0)
                    .top(100.0)
                    .size(50.0)
                    .on_click(move |_| hit_behind()),
            )
    });
    ui.click_at(20.0, 20.0);
    ui.run(0.6);
    assert_eq!(
        ui.bounds(30.0, 60.0)?,
        Rect::new(11.0, 39.0, 80.0, 80.0),
        "below the trigger, inside the panel border"
    );
    ui.click_at(30.0, 60.0);
    assert_eq!(inside.get(), 1);
    assert!(open.get(), "clicks inside keep it open");
    ui.click_at(220.0, 120.0);
    assert!(!open.get(), "an outside click dismisses it");
    assert_eq!(behind.get(), 0, "and is swallowed");
    ui.run(0.6);
    ui.click_at(220.0, 120.0);
    assert_eq!(behind.get(), 1, "the page is interactive again");

    ui.click_at(20.0, 20.0);
    ui.run(0.6);
    assert!(open.get());
    assert!(
        ui.press(Key::Escape),
        "focus moved into the popover, so escape reaches it"
    );
    assert!(!open.get());
    ui.run(0.6);
    assert!(ui.press(Key::Enter), "focus returned to the trigger");
    assert!(open.get());
    Ok(())
}

#[test]
fn popover_can_be_placed_on_any_side_of_its_trigger() -> TestResult {
    let _entered = Runtime::new().enter();
    let sides = [
        (Placement::Above, 51.0, 75.0),
        (Placement::Right, 115.0, 121.0),
        (Placement::Left, 5.0, 121.0),
    ];
    for (placement, x, y) in sides {
        let open = signal(true);
        let mut ui = builder().build(move || {
            let trigger = div().w(60.0).h(20.0);
            let anchored = popover(open, trigger, || div().size(40.0)).placement(placement);
            div()
                .size_full()
                .child(div().absolute().left(50.0).top(120.0).child(anchored))
        });
        ui.run(0.6);
        assert_eq!(
            ui.bounds(x + 20.0, y + 20.0)?,
            Rect::new(x, y, 40.0, 40.0),
            "{placement:?} sits a gap away from the trigger"
        );
    }
    Ok(())
}

#[test]
fn tooltip_appears_after_hovering_for_the_delay_and_hides_on_leave() {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(|| {
        let target = div().w(60.0).h(20.0);
        div()
            .size_full()
            .p(10.0)
            .child(tooltip(target, "Helpful").delay(Duration::from_millis(20)))
    });
    let shown = |ui: &mut TestUi| ui.stats().elements;
    let idle = shown(&mut ui);
    ui.pointer_moved(Point::new(20.0, 20.0));
    ui.advance(0.0);
    assert_eq!(shown(&mut ui), idle, "nothing yet");
    assert!(
        ui.wait_until(|ui| shown(ui) > idle),
        "the tooltip mounts after the delay"
    );
    ui.pointer_moved(Point::new(200.0, 150.0));
    assert!(
        ui.wait_until(|ui| shown(ui) == idle),
        "and unmounts after leaving"
    );
    ui.pointer_moved(Point::new(20.0, 20.0));
    ui.pointer_moved(Point::new(200.0, 150.0));
    sleep(Duration::from_millis(40));
    ui.run(0.6);
    assert_eq!(shown(&mut ui), idle, "leaving before the delay cancels it");
}

#[test]
fn tooltip_unmounted_while_pending_never_opens() {
    let _entered = Runtime::new().enter();
    let mounted = signal(true);
    let mut ui = builder().build(move || {
        div().size_full().p(10.0).child(show(
            move || mounted.get(),
            || tooltip(div().w(60.0).h(20.0), "Helpful").delay(Duration::from_millis(20)),
        ))
    });
    ui.pointer_moved(Point::new(20.0, 20.0));
    ui.advance(0.0);
    mounted.set(false);
    ui.advance(0.0);
    sleep(Duration::from_millis(40));
    ui.run(0.6);
}
