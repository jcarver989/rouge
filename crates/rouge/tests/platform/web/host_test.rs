#![cfg(target_arch = "wasm32")]

mod support;

use gloo_timers::future::TimeoutFuture;
use rouge::core::{Styled, div, editable, window_handle};
use rouge::reactive::{effect, signal, sleep, spawn};
use rouge::{App, WindowOptions};
use std::time::Duration;
use support::{click, container, document, drawn, until};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{ClipboardEvent, ClipboardEventInit, DataTransfer};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn a_mounted_app_sizes_its_canvas_and_answers_dom_pointer_events() -> Result<(), JsValue> {
    let document = document()?;
    let container = container(&document, "pointer-host")?;
    App::new()
        .window(
            WindowOptions::default()
                .title("mounted")
                .canvas("pointer-host"),
            || {
                let clicks = signal(0u32);
                div().size_full().on_click(move |_| {
                    clicks.update(|count| *count += 1);
                    window_handle().set_title(format!("clicked {}", clicks.get()));
                })
            },
        )
        .run()
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let canvas = drawn(&document, &container, "mounted").await?;
    let scale = web_sys::window().ok_or("no window")?.device_pixel_ratio();
    assert_eq!(canvas.height(), (100.0 * scale).round() as u32);
    click(&canvas)?;
    until("the click", || document.title() == "clicked 1").await
}

#[wasm_bindgen_test]
async fn a_dom_paste_lands_in_the_focused_editor() -> Result<(), JsValue> {
    let document = document()?;
    let container = container(&document, "paste-host")?;
    App::new()
        .window(
            WindowOptions::default().title("paste").canvas("paste-host"),
            || {
                let value = signal(String::new());
                let window = window_handle();
                effect(move || {
                    let text = value.get();
                    if !text.is_empty() {
                        window.set_title(format!("value {text}"));
                    }
                });
                div().size_full().child(editable(value).w_full().h(30.0))
            },
        )
        .run()
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let canvas = drawn(&document, &container, "paste").await?;
    click(&canvas)?;
    until("the text input focus", || {
        document
            .active_element()
            .is_some_and(|active| active.tag_name() == "TEXTAREA")
    })
    .await?;
    let input = document.active_element().ok_or("no focused input")?;
    let data = DataTransfer::new()?;
    data.set_data("text/plain", "from the page")?;
    let init = ClipboardEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_clipboard_data(Some(&data));
    let paste = ClipboardEvent::new_with_event_init_dict("paste", &init)?;
    input.dispatch_event(&paste)?;
    until("the pasted text", || {
        document.title() == "value from the page"
    })
    .await?;
    assert!(paste.default_prevented());
    Ok(())
}

#[wasm_bindgen_test]
async fn a_mounted_app_wakes_for_a_browser_timer() -> Result<(), JsValue> {
    let document = document()?;
    container(&document, "timer-host")?;
    let _mounted = App::new()
        .window(
            WindowOptions::default().title("timer").canvas("timer-host"),
            timed_title("woke"),
        )
        .mount()
        .await
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    until("the timer", || document.title() == "woke").await
}

#[wasm_bindgen_test]
async fn unmounting_cancels_pending_wakes() -> Result<(), JsValue> {
    let document = document()?;
    container(&document, "unmount-host")?;
    let mounted = App::new()
        .window(
            WindowOptions::default()
                .title("unmount")
                .canvas("unmount-host"),
            timed_title("too late"),
        )
        .mount()
        .await
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    until("the title", || document.title() == "unmount").await?;
    drop(mounted);
    TimeoutFuture::new(60).await;
    assert_eq!(document.title(), "unmount");
    Ok(())
}

fn timed_title(title: &'static str) -> impl FnOnce() -> rouge::core::Div {
    move || {
        let window = window_handle();
        spawn(async move {
            sleep(Duration::from_millis(30)).await;
            window.set_title(title);
        });
        div().size_full()
    }
}
