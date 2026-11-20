mod common;

use common::handheld_test;
use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{Edges, Point, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{NoticeTone, Toasts, button, tabs, toast};
use std::thread::sleep;
use std::time::Duration;

#[test]
fn toasts_stack_in_the_overlay_fan_out_on_hover_and_dismiss_themselves() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, toasts) = component_test().size(500.0, 400.0).build_with(|| {
        let toasts = Toasts::install();
        let save = view! {
            <button label="Save" test_id="save"
                 on_click={|| {
                     toast("Saved")
                         .tone(NoticeTone::Positive)
                         .duration(Duration::from_millis(60))
                         .show();
                 }}/>
        };
        (
            view! {
                <div size_full p=10.0>
                    {save}
                    {toasts}
                </div>
            },
            toasts,
        )
    });
    ui.click_on("save")?;
    ui.click_on("save")?;
    ui.run(0.5);
    assert_eq!(toasts.len(), 2, "a handler reaches its window's queue");
    let (older, newer) = (ui.rect_of("toast-0")?, ui.rect_of("toast-1")?);
    assert!(newer.max().x <= 500.0 && newer.max().y <= 400.0 && newer.origin.x > 100.0);
    assert!(
        older.size.width < newer.size.width && older.origin.y < newer.origin.y,
        "the older toast peeks out behind the newer one"
    );
    ui.pointer_moved(newer.center());
    ui.run(0.5);
    let (older, newer) = (ui.rect_of("toast-0")?, ui.rect_of("toast-1")?);
    assert_eq!(older.size, newer.size);
    assert!(
        older.max().y <= newer.origin.y,
        "hovering fans the stack out"
    );
    let overlap = ui.stats().elements;
    sleep(Duration::from_millis(120));
    ui.run(0.5);
    assert_eq!(toasts.len(), 2, "hovering pauses the timers");
    assert_eq!(ui.stats().elements, overlap);
    ui.pointer_moved(Point::new(5.0, 5.0));
    assert!(
        ui.wait_until(|_| toasts.is_empty()),
        "they dismiss themselves"
    );
    ui.run(1.0);
    assert!(ui.bounds_of("toast-0").is_none() && ui.bounds_of("toast-1").is_none());
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

#[test]
fn a_toast_action_runs_and_dismisses_it() -> TestResult {
    let _entered = Runtime::new().enter();
    let undone = signal(false);
    let (mut ui, toasts) = component_test().size(500.0, 400.0).build_with(|| {
        let toasts = Toasts::install();
        (view! { <div size_full>{toasts}</div> }, toasts)
    });
    toasts.show(
        toast("3 albums removed")
            .detail("Downloads were deleted from this device.")
            .action("Undo", move || undone.set(true)),
    );
    ui.run(0.5);
    let card = ui.rect_of("toast-0")?;
    let detail = ui
        .text_bounds("Downloads were deleted from this device.")
        .ok_or("the detail is drawn")?;
    assert!(card.contains(detail.center()));
    ui.click_on("toast-0-action")?;
    assert!(undone.get());
    assert!(toasts.is_empty());
    Ok(())
}

#[test]
fn on_a_handheld_one_toast_spans_the_window_above_the_tab_bar_and_swipes_away() -> TestResult {
    let _entered = Runtime::new().enter();
    let tab = signal(0usize);
    let (mut ui, toasts) = handheld_test().build_with(move || {
        let toasts = Toasts::install();
        let page = view! {
            <tabs selected=tab test_id="tabs" bar tab=("Inbox", || view! { <div/> }) tab=("You", || view! { <div/> })
                 size_full/>
        };
        (view! { <div size_full>{page}{toasts}</div> }, toasts)
    });
    ui.run(1.0);
    toasts.show(toast("Archived").duration(Duration::from_secs(30)));
    toasts.show(toast("Moved").duration(Duration::from_secs(30)));
    ui.run(1.0);
    let (bar, newest) = (ui.rect_of("tabs-bar")?, ui.rect_of("toast-1")?);
    assert_eq!((newest.origin.x, newest.size.width), (12.0, 366.0));
    assert_eq!(newest.max().y, bar.origin.y - 8.0, "it clears the bar");
    assert_eq!(
        ui.rect_of("toast-0")?.size,
        newest.size,
        "the older toast waits behind instead of peeking out"
    );
    let at = newest.center();
    ui.swipe_over(at, Point::new(at.x + 200.0, at.y), 0.2);
    ui.run(1.0);
    assert_eq!(toasts.len(), 1, "a sideways swipe dismisses it");
    assert_eq!(ui.bounds_of("toast-1"), None);
    let at = ui.rect_of("toast-0")?.center();
    ui.swipe_over(at, Point::new(at.x + 20.0, at.y), 0.2);
    ui.run(1.0);
    assert_eq!(toasts.len(), 1, "a short drag springs back");
    assert_eq!(ui.rect_of("toast-0")?.center(), at);
    Ok(())
}

#[test]
fn a_banner_drops_from_the_top_safe_area_and_leaves_the_bottom_stack_alone() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, toasts) = handheld_test().build_with(|| {
        let toasts = Toasts::install();
        (view! { <div size_full>{toasts}</div> }, toasts)
    });
    ui.set_safe_area(Edges {
        top: 48.0,
        ..Edges::ZERO
    });
    toasts.show(
        toast("Build failed")
            .tone(NoticeTone::Error)
            .banner()
            .duration(Duration::from_secs(30)),
    );
    toasts.show(toast("Saved").duration(Duration::from_secs(30)));
    ui.run(1.0);
    let (banner, saved) = (ui.rect_of("toast-0")?, ui.rect_of("toast-1")?);
    assert_eq!(
        (banner.origin.x, banner.origin.y, banner.size.width),
        (12.0, 56.0, 366.0)
    );
    assert_eq!(saved.max().y, 700.0 - 8.0);
    let at = banner.center();
    ui.swipe_over(at, Point::new(at.x, at.y - 80.0), 0.2);
    ui.run(1.0);
    assert_eq!(ui.bounds_of("toast-0"), None, "a banner is pushed back up");
    assert_eq!(toasts.len(), 1);
    Ok(())
}
