#![cfg(target_arch = "wasm32")]

mod support;

use js_sys::Promise;
use rouge::core::{Styled, div, window_handle};
use rouge::reactive::signal;
use rouge::{App, WindowOptions};
use support::{click, container, document, drawn, next_frame, until};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn a_page_asking_for_the_gl_backend_draws_and_takes_input_through_webgl2()
-> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let history = window.history()?;
    history.replace_state_with_url(&JsValue::NULL, "", Some("?rouge_backend=gl"))?;
    let document = document()?;
    let container = container(&document, "webgl-host")?;
    App::new()
        .window(
            WindowOptions::default().title("webgl").canvas("webgl-host"),
            || {
                let clicks = signal(0u32);
                div()
                    .size_full()
                    .bg(move || {
                        if clicks.get() == 0 {
                            0xff0000u32
                        } else {
                            0x0000ffu32
                        }
                    })
                    .on_click(move |_| {
                        clicks.update(|count| *count += 1);
                        window_handle().set_title(format!("clicked {}", clicks.get()));
                    })
            },
        )
        .run()
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let canvas = drawn(&document, &container, "webgl").await?;
    assert!(canvas.get_context("webgl2")?.is_some());
    click(&canvas)?;
    until("the click", || document.title() == "clicked 1").await?;
    JsFuture::from(Promise::resolve(&JsValue::NULL)).await?;
    next_frame().await?;
    assert_eq!(pixel(&canvas)?, [0, 0, 255, 255]);
    Ok(())
}

fn pixel(canvas: &HtmlCanvasElement) -> Result<[u8; 4], JsValue> {
    let copy: HtmlCanvasElement = document()?.create_element("canvas")?.dyn_into()?;
    copy.set_width(1);
    copy.set_height(1);
    let context: CanvasRenderingContext2d = copy.get_context("2d")?.ok_or("no 2d")?.dyn_into()?;
    let (x, y) = (
        f64::from(canvas.width() / 2),
        f64::from(canvas.height() / 2),
    );
    context.draw_image_with_html_canvas_element(canvas, -x, -y)?;
    let data = context.get_image_data(0.0, 0.0, 1.0, 1.0)?.data();
    Ok([data[0], data[1], data[2], data[3]])
}
