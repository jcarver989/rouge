use super::SystemEngine;
use super::shared::document::document;
use crate::content::WebContent;
use crate::engine::{
    EnginePage, PageCommand, Placement, WebEngine, WebViewEvent, WebViewEvents, WebViewOptions,
};
use crate::error::WebViewError;
use gloo_events::EventListener;
use js_sys::JSON;
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Event, HtmlCanvasElement, HtmlElement, HtmlIFrameElement, MessageEvent, Window};

const CONTAINER_STYLE: &str = "position:fixed;overflow:hidden;display:none;margin:0;padding:0;";
const FRAME_STYLE: &str =
    "position:absolute;border:0;margin:0;padding:0;transform-origin:0 0;background:#fff;";
const SANDBOX: &str = "allow-scripts allow-forms allow-popups allow-modals allow-downloads";
const RELAYOUT_EVENTS: [&str; 2] = ["resize", "scroll"];

impl WebEngine for SystemEngine<HtmlCanvasElement> {
    fn open(
        &self,
        _options: WebViewOptions,
        events: WebViewEvents,
    ) -> Result<Box<dyn EnginePage>, WebViewError> {
        Page::open(&self.0, events)
            .map(|page| Box::new(page) as Box<dyn EnginePage>)
            .map_err(|error| WebViewError::Create(format!("{error:?}")))
    }
}

struct Dom {
    canvas: HtmlCanvasElement,
    container: HtmlElement,
    frame: HtmlIFrameElement,
    placement: Cell<Placement>,
}

impl Dom {
    fn lay_out(&self) {
        let style = self.container.style();
        let Some(layout) = self.placement.get().layout(1.0) else {
            let _ = style.set_property("display", "none");
            return;
        };
        let origin = self.canvas.get_bounding_client_rect();
        let (clip, page, corners) = (layout.clip, layout.page, layout.corners);
        let unzoomed = layout.unzoomed;
        let pointer = if layout.interactive { "auto" } else { "none" };
        let pixels = |value: f64| format!("{value}px");
        let container = [
            ("display", "block".to_owned()),
            ("left", pixels(origin.left() + f64::from(clip.origin.x))),
            ("top", pixels(origin.top() + f64::from(clip.origin.y))),
            ("width", pixels(f64::from(clip.size.width))),
            ("height", pixels(f64::from(clip.size.height))),
            ("pointer-events", pointer.to_owned()),
            (
                "border-radius",
                format!(
                    "{}px {}px {}px {}px",
                    corners.top_left, corners.top_right, corners.bottom_right, corners.bottom_left
                ),
            ),
        ];
        for (property, value) in container {
            let _ = style.set_property(property, &value);
        }
        let inner = [
            ("left", pixels(f64::from(page.origin.x))),
            ("top", pixels(f64::from(page.origin.y))),
            ("width", pixels(f64::from(unzoomed.width))),
            ("height", pixels(f64::from(unzoomed.height))),
            ("transform", format!("scale({})", layout.zoom)),
            ("pointer-events", pointer.to_owned()),
        ];
        let style = self.frame.style();
        for (property, value) in inner {
            let _ = style.set_property(property, &value);
        }
    }
}

struct Page {
    dom: Rc<Dom>,
    events: WebViewEvents,
    content: Option<WebContent>,
    _listeners: Vec<EventListener>,
}

impl Page {
    fn open(canvas: &HtmlCanvasElement, events: WebViewEvents) -> Result<Self, JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let container: HtmlElement = document.create_element("div")?.dyn_into()?;
        container.set_attribute("style", CONTAINER_STYLE)?;
        let frame: HtmlIFrameElement = document.create_element("iframe")?.dyn_into()?;
        frame.set_attribute("style", FRAME_STYLE)?;
        container.append_child(&frame)?;
        let host = canvas
            .parent_element()
            .or_else(|| document.body().map(Into::into))
            .ok_or("the canvas is not in the document")?;
        let dom = Rc::new(Dom {
            canvas: canvas.clone(),
            container,
            frame,
            placement: Cell::new(Placement::default()),
        });
        let mut listeners: Vec<EventListener> = RELAYOUT_EVENTS
            .into_iter()
            .map(|name| {
                let dom = dom.clone();
                EventListener::new(&window, name, move |_| dom.lay_out())
            })
            .collect();
        let loaded = events.clone();
        listeners.push(EventListener::new(&dom.frame, "load", move |_| {
            loaded.send(WebViewEvent::Progress(1.0));
            loaded.send(WebViewEvent::Loading(false));
        }));
        let (messages, source) = (events.clone(), dom.clone());
        listeners.push(EventListener::new(&window, "message", move |event| {
            if let Some(text) = message_from(&source.frame, event) {
                messages.send(WebViewEvent::Message(text));
            }
        }));
        host.append_child(&dom.container)?;
        Ok(Page {
            dom,
            events,
            content: None,
            _listeners: listeners,
        })
    }

    fn show(&self, content: &WebContent) {
        let frame = &self.dom.frame;
        self.events.send(WebViewEvent::Progress(0.0));
        self.events.send(WebViewEvent::Loading(true));
        match content {
            WebContent::Url(url) => {
                let _ = frame.remove_attribute("srcdoc");
                let _ = frame.remove_attribute("sandbox");
                frame.set_src(url);
                self.events.send(WebViewEvent::Url(url.clone()));
            }
            WebContent::Html { html, base_url } => {
                let _ = frame.remove_attribute("src");
                let _ = frame.set_attribute("sandbox", SANDBOX);
                frame.set_srcdoc(&document(html, base_url.as_deref()));
                self.events
                    .send(WebViewEvent::Url(base_url.clone().unwrap_or_default()));
            }
        }
    }
}

fn message_from(frame: &HtmlIFrameElement, event: &Event) -> Option<String> {
    let event = event.dyn_ref::<MessageEvent>()?;
    let (source, window) = (event.source()?, frame.content_window()?);
    if JsValue::from(source) != JsValue::from(window) {
        return None;
    }
    let data = event.data();
    data.as_string().or_else(|| {
        JSON::stringify(&data)
            .ok()
            .and_then(|text| text.as_string())
    })
}

impl EnginePage for Page {
    fn apply(&mut self, command: PageCommand) {
        match command {
            PageCommand::Load(content) => {
                self.show(&content);
                self.content = Some(content);
            }
            PageCommand::Place(placement) => {
                let _ = (self.dom.container.style())
                    .set_property("opacity", &placement.opacity.to_string());
                self.dom.container.set_inert(!placement.interactive);
                self.dom.placement.set(placement);
                self.dom.lay_out();
            }
            PageCommand::Reload => {
                if let Some(content) = &self.content {
                    self.show(content);
                }
            }
            PageCommand::Post(message) => {
                if let Some(window) = self.dom.frame.content_window() {
                    let _ = Window::post_message(&window, &JsValue::from_str(&message), "*");
                }
            }
            PageCommand::Stop | PageCommand::Back | PageCommand::Forward => {}
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        self.dom.container.remove();
    }
}
