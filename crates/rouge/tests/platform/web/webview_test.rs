#![cfg(target_arch = "wasm32")]

mod support;

use rouge::core::{Corners, Styled, div, view, window_handle};
use rouge::reactive::signal;
use rouge::webview::{WebContent, web_page, web_view};
use rouge::{App, WindowOptions};
use support::{click, container, document, drawn, until};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{Element, HtmlElement};

wasm_bindgen_test_configure!(run_in_browser);

const PAGE: &str = "<body><script>\
addEventListener('message', (event) => parent.postMessage('echo ' + event.data, '*'));\
parent.postMessage('ready', '*');\
</script></body>";

#[wasm_bindgen_test]
async fn a_web_view_is_an_iframe_over_the_canvas_that_talks_to_the_app() -> Result<(), JsValue> {
    let document = document()?;
    let host = container(&document, "web-view-host")?;
    App::new()
        .window(
            WindowOptions::default()
                .title("web view")
                .canvas("web-view-host"),
            || {
                let awake = signal(false);
                let page = web_page(WebContent::html(PAGE));
                page.on_message(move |message: String| {
                    if message == "ready" {
                        awake.set(true);
                        page.post_message("hello");
                    }
                    window_handle().set_title(message);
                });
                view! {
                    <div size_full p=10.0 on_click={move |_| awake.set(false)}>
                        <web_view page interactive={move || awake.get()} page_zoom=0.5 w=120.0 h=60.0
                             rounded={Corners::all(6.0)}/>
                    </div>
                }
            },
        )
        .run()
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let canvas = drawn(&document, &host, "echo hello").await?;
    let frame: HtmlElement = host
        .query_selector("iframe")?
        .ok_or("no iframe beside the canvas")?
        .dyn_into()?;
    let clip: Element = frame.parent_element().ok_or("no clip around the iframe")?;
    let (origin, placed) = (
        canvas.get_bounding_client_rect(),
        clip.get_bounding_client_rect(),
    );
    assert_eq!(
        (
            placed.left() - origin.left(),
            placed.top() - origin.top(),
            placed.width(),
            placed.height()
        ),
        (10.0, 10.0, 120.0, 60.0)
    );
    let style = web_sys::window()
        .ok_or("no window")?
        .get_computed_style(&clip)?
        .ok_or("no style")?;
    assert_eq!(style.get_property_value("border-top-left-radius")?, "6px");
    assert_eq!(style.get_property_value("pointer-events")?, "auto");
    assert_eq!(
        (frame.offset_width(), frame.offset_height()),
        (240, 120),
        "the page lays out at twice the size and is scaled by half"
    );
    assert!(
        frame
            .get_attribute("sandbox")
            .is_some_and(|sandbox| !sandbox.contains("allow-same-origin")),
        "inline HTML must not share the app's origin"
    );
    click(&canvas)?;
    until("the sleeping iframe", || clip.has_attribute("inert")).await?;
    assert_eq!(style.get_property_value("pointer-events")?, "none");
    Ok(())
}
