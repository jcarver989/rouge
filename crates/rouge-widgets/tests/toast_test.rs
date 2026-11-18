use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{Point, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{NoticeTone, Toasts, button, toast};
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
