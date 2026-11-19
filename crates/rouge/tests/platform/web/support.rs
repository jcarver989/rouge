use gloo_timers::future::TimeoutFuture;
use js_sys::Promise;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Document, Element, Event, HtmlCanvasElement, PointerEvent, PointerEventInit};

pub(crate) fn document() -> Result<Document, JsValue> {
    web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "no document".into())
}

pub(crate) fn container(document: &Document, id: &str) -> Result<Element, JsValue> {
    let container = document.create_element("div")?;
    container.set_id(id);
    container.set_attribute("style", "width:200px;height:100px;")?;
    document.body().ok_or("no body")?.append_child(&container)?;
    Ok(container)
}

pub(crate) async fn drawn(
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

pub(crate) async fn next_frame() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let mut requested = Ok(0);
    let frame = Promise::new(&mut |resolve, _| {
        requested = window.request_animation_frame(&resolve);
    });
    requested?;
    JsFuture::from(frame).await.map(drop)
}

pub(crate) fn click(canvas: &Element) -> Result<(), JsValue> {
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

pub(crate) async fn until(what: &str, mut done: impl FnMut() -> bool) -> Result<(), JsValue> {
    for _ in 0..1000 {
        if done() {
            return Ok(());
        }
        TimeoutFuture::new(10).await;
    }
    Err(format!("timed out waiting for {what}").into())
}
