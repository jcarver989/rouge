#![cfg(target_arch = "wasm32")]

use gloo_timers::future::TimeoutFuture;
use rouge_core::{Point, Rect, Size, Styled, Ui, div, image, text};
use rouge_reactive::{poll_tasks, sleep, spawn};
use rouge_web::BrowserRuntime;
use std::cell::Cell;
use std::rc::{Rc, Weak};
use std::time::Duration;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn idle_runtime_wakes_for_a_browser_timer() -> Result<(), &'static str> {
    let woke = Rc::new(Cell::new(0));
    let runtime = Rc::new_cyclic(|runtime: &Weak<BrowserRuntime>| {
        let runtime = runtime.clone();
        let woke = woke.clone();
        BrowserRuntime::new(move || {
            woke.set(woke.get() + 1);
            if let Some(runtime) = runtime.upgrade() {
                let _entered = runtime.enter();
                poll_tasks();
            }
        })
    });
    let (task, other) = {
        let _entered = runtime.enter();
        let task = spawn(async {
            sleep(Duration::from_millis(10)).await;
            42
        });
        (task, spawn(async { 7 }))
    };
    assert_eq!(woke.get(), 0, "wake runs in a later microtask");
    assert_eq!(other.await.map_err(|_| "other task cancelled")?, 7);
    assert_eq!(woke.get(), 1, "initial wakes coalesce");
    assert_eq!(task.await.map_err(|_| "timer task cancelled")?, 42);
    assert_eq!(woke.get(), 2, "timer wakes an idle runtime");
    Ok(())
}

#[wasm_bindgen_test]
async fn dropping_the_runtime_discards_a_queued_wake_and_cancels_a_timer() {
    let woke = Rc::new(Cell::new(0));
    let runtime = BrowserRuntime::new({
        let woke = woke.clone();
        move || woke.set(woke.get() + 1)
    });
    {
        let _entered = runtime.enter();
        spawn(sleep(Duration::from_millis(10)));
        poll_tasks();
    }
    drop(runtime);
    TimeoutFuture::new(30).await;
    assert_eq!(woke.get(), 0);
}

#[wasm_bindgen_test]
fn bundled_font_shapes_on_the_first_ui_frame() {
    let runtime = BrowserRuntime::new(|| {});
    let _entered = runtime.enter();
    let font = include_bytes!("fixtures/inter-subset.ttf");
    let mut ui = Ui::with_fonts(Size::new(160.0, 40.0), [font.to_vec()], || {
        text("Rouge").font("Inter")
    });
    assert!(ui.update());
    assert_eq!(ui.text_index_at(Point::new(10.0, 8.0)), Some(1));
}

#[wasm_bindgen_test]
fn byte_backed_images_decode_without_a_native_worker() {
    let runtime = BrowserRuntime::new(|| {});
    let _entered = runtime.enter();
    let mut ui = Ui::new(Size::new(160.0, 40.0), || {
        div()
            .flex()
            .items_start()
            .child(image(include_bytes!("fixtures/red.png").to_vec()).flex_none())
    });
    assert!(ui.update());
    assert_eq!(
        ui.bounds_at(Point::new(1.0, 0.5)),
        Some(Rect::new(0.0, 0.0, 2.0, 1.0))
    );
}
