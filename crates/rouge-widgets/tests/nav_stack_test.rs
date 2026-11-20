mod common;

use common::handheld_test;
use rouge_core::testing::{TestResult, TestUi};
use rouge_core::view;
use rouge_core::{Point, Styled, WindowHandle, div, route, window_handle};
use rouge_reactive::{Runtime, Signal, effect, on_cleanup, signal};
use rouge_widgets::{NavParams, nav_bar, nav_stack};

#[test]
fn a_path_stacks_the_screen_of_each_matching_prefix_and_passes_its_parameters() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, app) = App::open();
    ui.run(0.5);
    assert_eq!(ui.rect_of("stack-page-/")?.size.width, 390.0);
    assert_eq!(ui.bounds_of("thread"), None, "only the root is shown at /");
    app.window.navigate("/thread/42");
    ui.advance(0.0);
    let page = ui.rect_of("stack-page-/thread/42")?;
    assert!(
        page.origin.x > 300.0,
        "the new screen starts off the trailing edge"
    );
    assert!(ui.is_animating());
    ui.run(1.0);
    assert_eq!(ui.rect_of("stack-page-/thread/42")?.origin.x, 0.0);
    assert_eq!(app.opened.get(), "42", "the page hears its parameter");
    assert_eq!(
        ui.rect_of("stack-page-/")?.origin.x,
        0.0,
        "the screen beneath stays mounted, and its layout does not move"
    );
    assert_eq!(app.mounted.get(), 1, "so it keeps its state");
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    app.window.navigate("/settings");
    ui.run(1.0);
    assert_eq!(ui.bounds_of("thread"), None);
    assert!(
        ui.bounds_of("settings").is_some(),
        "a sibling replaces the top"
    );
    Ok(())
}

#[test]
fn the_back_button_and_escape_pop_to_the_screen_beneath() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, app) = App::open();
    assert_eq!(
        ui.bounds_of("root-bar-back"),
        None,
        "the root has nowhere to go"
    );
    app.window.navigate("/thread/7");
    ui.run(1.0);
    ui.tap_on("thread-bar-back")?;
    assert_eq!(app.window_path(), "/");
    ui.run(1.0);
    assert_eq!(ui.bounds_of("thread"), None, "the popped screen is gone");
    app.window.navigate("/thread/7");
    ui.run(1.0);
    ui.press("escape");
    assert_eq!(
        app.window_path(),
        "/",
        "Escape, which is Back on Android, pops too"
    );
    ui.press("escape");
    assert_eq!(app.window_path(), "/", "and leaves the root alone");
    Ok(())
}

#[test]
fn a_swipe_from_the_leading_edge_scrubs_the_screen_and_pops_it_past_halfway() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, app) = App::open();
    app.window.navigate("/thread/7");
    ui.run(1.0);
    ui.swipe_over(Point::new(200.0, 300.0), Point::new(380.0, 300.0), 0.3);
    ui.run(1.0);
    assert_eq!(
        app.window_path(),
        "/thread/7",
        "a drag from the middle is not a back swipe"
    );
    ui.swipe_over(Point::new(8.0, 300.0), Point::new(80.0, 300.0), 0.4);
    ui.run(1.0);
    assert_eq!(app.window_path(), "/thread/7", "a short swipe springs back");
    assert_eq!(ui.rect_of("stack-page-/thread/7")?.origin.x, 0.0);
    ui.swipe_over(Point::new(8.0, 300.0), Point::new(300.0, 300.0), 0.3);
    assert_eq!(app.window_path(), "/", "past halfway it pops");
    ui.run(1.0);
    assert_eq!(ui.bounds_of("thread"), None);
    Ok(())
}

#[derive(Clone, Copy)]
struct App {
    window: WindowHandle,
    opened: Signal<&'static str>,
    mounted: Signal<u32>,
    path: Signal<String>,
}

impl App {
    fn open() -> (TestUi, App) {
        handheld_test().build_with(|| {
            let app = App {
                window: window_handle(),
                opened: signal(""),
                mounted: signal(0),
                path: signal(String::new()),
            };
            let route = route();
            effect(move || app.path.set(route.get()));
            let root = move |_| {
                app.mounted.update(|count| *count += 1);
                on_cleanup(move || app.mounted.update(|count| *count -= 1));
                view! {
                    <div flex col size_full>
                        <nav_bar title="Inbox" test_id="root-bar"/>
                    </div>
                }
            };
            let thread = move |params: NavParams| {
                app.opened.set(match params.get("id") {
                    Some("42") => "42",
                    Some("7") => "7",
                    _ => "?",
                });
                view! {
                    <div test_id="thread" flex col size_full>
                        <nav_bar title="Thread" test_id="thread-bar"/>
                    </div>
                }
            };
            let stack = view! {
                <nav_stack test_id="stack" page=("/", root) page=("/thread/:id", thread)
                     page=("/settings", |_| view! { <div test_id="settings" size_full/> }) size_full/>
            };
            (stack, app)
        })
    }

    fn window_path(self) -> String {
        match self.path.get().as_str() {
            "" => "/".to_owned(),
            path => path.to_owned(),
        }
    }
}
