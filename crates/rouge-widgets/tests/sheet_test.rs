use rouge_core::testing::{TestError, TestResult, TestUi, counter};
use rouge_core::view;
use rouge_core::{Point, Styled, div};
use rouge_reactive::{Runtime, Signal, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{Detent, sheet};

#[test]
fn a_sheet_rises_to_its_first_detent_and_then_idles() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, open) = queue([Detent::Medium, Detent::Large]);
    assert_eq!(
        ui.bounds_of("queue"),
        None,
        "nothing is mounted while closed"
    );
    open.set(true);
    ui.advance(0.0);
    assert!(ui.is_animating(), "it slides in");
    ui.run(2.0);
    assert_eq!(top(&mut ui)?, 400.0, "the medium detent is half the window");
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

#[test]
fn a_drag_settles_on_the_nearest_detent_and_a_flick_carries_it_further() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, open) = queue([Detent::Medium, Detent::Large]);
    open.set(true);
    ui.run(2.0);
    ui.swipe_over(Point::new(200.0, 410.0), Point::new(200.0, 380.0), 1.0);
    ui.run(2.0);
    assert_eq!(top(&mut ui)?, 400.0, "a short slow drag springs back");
    ui.swipe_over(Point::new(200.0, 410.0), Point::new(200.0, 100.0), 1.0);
    ui.run(2.0);
    assert_eq!(
        top(&mut ui)?,
        8.0,
        "dragged most of the way, it opens fully"
    );
    ui.swipe_over(Point::new(200.0, 20.0), Point::new(200.0, 80.0), 0.05);
    ui.run(2.0);
    assert_eq!(
        top(&mut ui)?,
        400.0,
        "a quick flick down drops a detent although it moved only a little"
    );
    ui.swipe_over(Point::new(200.0, 410.0), Point::new(200.0, 700.0), 0.3);
    ui.run(2.0);
    assert!(!open.get(), "dragged off the bottom, it closes");
    assert_eq!(ui.bounds_of("queue"), None);
    Ok(())
}

#[test]
fn the_scrim_and_escape_dismiss_without_reaching_the_page() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(false);
    let (behind, hit_behind) = counter();
    let mut ui = component_test().size(400.0, 800.0).build(move || {
        view! {
            <div size_full on_click={move |_| hit_behind()}>
                <sheet open=open test_id="queue">
                    <div h=120.0/>
                </sheet>
            </div>
        }
    });
    open.set(true);
    ui.run(2.0);
    ui.click_at(200.0, 100.0);
    ui.run(2.0);
    assert_eq!((open.get(), behind.get()), (false, 0), "a scrim tap closes");
    open.set(true);
    ui.run(2.0);
    ui.press("escape");
    ui.run(2.0);
    assert!(!open.get(), "so does escape");
    Ok(())
}

#[test]
fn a_sheet_without_detents_hugs_its_content() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(true);
    let mut ui = component_test().size(400.0, 800.0).build(move || {
        view! {
            <sheet open=open test_id="queue">
                <div h=120.0/>
            </sheet>
        }
    });
    ui.run(2.0);
    assert_eq!(
        top(&mut ui)?,
        800.0 - 120.0 - 20.0,
        "content plus the grabber"
    );
    Ok(())
}

#[test]
fn the_grabber_moves_between_detents_from_the_keyboard() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, open) = queue([Detent::Medium, Detent::Large]);
    open.set(true);
    ui.run(2.0);
    ui.press("tab");
    ui.press("up");
    ui.run(2.0);
    assert_eq!(top(&mut ui)?, 8.0);
    ui.press("down");
    ui.run(2.0);
    assert_eq!(top(&mut ui)?, 400.0);
    ui.press("down");
    ui.run(2.0);
    assert!(!open.get(), "down from the lowest detent closes");
    Ok(())
}

fn queue(detents: [Detent; 2]) -> (TestUi, Signal<bool>) {
    let open = signal(false);
    let ui = component_test().size(400.0, 800.0).build(move || {
        view! {
            <div size_full>
                <sheet open=open detents=detents test_id="queue">
                    <div flex_1/>
                </sheet>
            </div>
        }
    });
    (ui, open)
}

fn top(ui: &mut TestUi) -> Result<f32, TestError> {
    Ok(ui.rect_of("queue")?.origin.y.round())
}
