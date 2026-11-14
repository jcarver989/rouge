#![cfg(target_arch = "wasm32")]

use gloo_timers::future::TimeoutFuture;
use js_sys::Promise;
use rouge::core::{Styled, div, editable, window_handle};
use rouge::reactive::{effect, signal};
use rouge::{App, WindowOptions};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{
    ClipboardEvent, ClipboardEventInit, DataTransfer, Document, Element, Event, HtmlCanvasElement,
    PointerEvent, PointerEventInit,
};

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

fn document() -> Result<Document, JsValue> {
    web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "no document".into())
}

fn container(document: &Document, id: &str) -> Result<Element, JsValue> {
    let container = document.create_element("div")?;
    container.set_id(id);
    container.set_attribute("style", "width:200px;height:100px;")?;
    document.body().ok_or("no body")?.append_child(&container)?;
    Ok(container)
}

async fn drawn(
    document: &Document,
    container: &Element,
    title: &str,
) -> Result<HtmlCanvasElement, JsValue> {
    until("the title", || document.title() == title).await?;
    let canvas: HtmlCanvasElement = container
        .first_element_child()
        .ok_or("no canvas")?
        .dyn_into()?;
    next_frame().await?;
    let scale = web_sys::window().ok_or("no window")?.device_pixel_ratio();
    assert_eq!(canvas.width(), (200.0 * scale).round() as u32);
    Ok(canvas)
}

async fn next_frame() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let mut requested = Ok(0);
    let frame = Promise::new(&mut |resolve, _| {
        requested = window.request_animation_frame(&resolve);
    });
    requested?;
    JsFuture::from(frame).await.map(drop)
}

fn click(canvas: &Element) -> Result<(), JsValue> {
    for name in ["pointerdown", "pointerup"] {
        canvas.dispatch_event(&pointer(canvas, name)?)?;
    }
    Ok(())
}

fn pointer(canvas: &Element, name: &str) -> Result<Event, JsValue> {
    let rect = canvas.get_bounding_client_rect();
    let init = PointerEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_pointer_id(1);
    init.set_pointer_type("mouse");
    init.set_is_primary(true);
    init.set_client_x((rect.left() + 20.0) as i32);
    init.set_client_y((rect.top() + 15.0) as i32);
    init.set_button(0);
    PointerEvent::new_with_event_init_dict(name, &init).map(Into::into)
}

async fn until(what: &str, mut done: impl FnMut() -> bool) -> Result<(), JsValue> {
    for _ in 0..1000 {
        if done() {
            return Ok(());
        }
        TimeoutFuture::new(10).await;
    }
    Err(format!("timed out waiting for {what}").into())
}
