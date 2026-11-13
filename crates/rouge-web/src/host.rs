use crate::BrowserRuntime;
use rouge_core::{
    Clipboard, Color, Cursor, ImeEvent, Key, KeyEvent, Modifiers, Point, PointerButton, Size, Ui,
    View, WindowCommand,
};
use rouge_render::{Gpu, Renderer, Surfaces, Viewport};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
use thiserror::Error;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    ClipboardEvent, CompositionEvent, Event, EventTarget, HtmlCanvasElement, HtmlTextAreaElement,
    InputEvent, KeyboardEvent, PointerEvent, ResizeObserver, WheelEvent, Window,
};
use web_time::Instant;
use wgpu::{
    CurrentSurfaceTexture, Instance, Surface, SurfaceConfiguration, SurfaceTarget,
    TextureViewDescriptor,
};

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("browser operation failed: {0:?}")]
    Dom(JsValue),
    #[error(transparent)]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error(transparent)]
    Gpu(#[from] rouge_render::GpuError),
    #[error("no supported canvas surface configuration")]
    UnsupportedSurface,
}

impl From<JsValue> for BrowserError {
    fn from(value: JsValue) -> Self {
        Self::Dom(value)
    }
}

type Handler = Closure<dyn FnMut(Event)>;

struct Listener(EventTarget, &'static str, Handler);
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self
            .0
            .remove_event_listener_with_callback(self.1, self.2.as_ref().unchecked_ref());
    }
}

struct BrowserClipboard;
impl Clipboard for BrowserClipboard {
    fn read(&mut self) -> Option<String> {
        None
    }
    fn write(&mut self, text: &str) {
        let text = text.to_owned();
        spawn_local(async move {
            if let Err(error) = copy_text(&text).await {
                web_sys::console::warn_1(&error);
            }
        });
    }
}

pub async fn copy_text(text: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no browser window"))?;
    JsFuture::from(window.navigator().clipboard().write_text(text)).await?;
    Ok(())
}

struct Host {
    window: Window,
    canvas: HtmlCanvasElement,
    input: HtmlTextAreaElement,
    surface: Surface<'static>,
    config: SurfaceConfiguration,
    gpu: Gpu,
    renderer: Renderer,
    surfaces: Surfaces,
    ui: RefCell<Ui>,
    runtime: BrowserRuntime,
    background: Color,
    frame: Cell<Option<i32>>,
    callback: Closure<dyn FnMut(f64)>,
    composing: Cell<bool>,
    frames: Cell<u64>,
}

pub struct BrowserApp {
    host: Rc<RefCell<Host>>,
    listeners: Vec<Listener>,
    observer: ResizeObserver,
    _resize: Closure<dyn FnMut()>,
}

impl BrowserApp {
    pub async fn mount<T: View>(
        canvas: HtmlCanvasElement,
        fonts: impl IntoIterator<Item = Vec<u8>>,
        background: Color,
        root: impl FnOnce() -> T,
    ) -> Result<Self, BrowserError> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no browser window"))?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("no browser document"))?;
        let instance = Instance::default();
        let surface = instance.create_surface(SurfaceTarget::Canvas(canvas.clone()))?;
        let gpu = Gpu::new(instance, Some(&surface)).await?;
        let config = surface
            .get_default_config(&gpu.adapter, 1, 1)
            .ok_or(BrowserError::UnsupportedSurface)?;
        let renderer = Renderer::new(&gpu.device, &gpu.queue, config.format);
        let surfaces = Surfaces::new(&gpu);
        let input = document
            .create_element("textarea")?
            .dyn_into::<HtmlTextAreaElement>()
            .map_err(JsValue::from)?;
        input.set_attribute("aria-label", "Rouge text input")?;
        input.set_attribute("autocapitalize", "off")?;
        input.set_attribute("autocomplete", "off")?;
        input.set_attribute("spellcheck", "false")?;
        input.set_tab_index(-1);
        input.set_attribute("style", "position:fixed;opacity:0;pointer-events:none;width:1px;height:1px;resize:none;font-size:16px;")?;
        canvas.set_tab_index(0);
        let rect = canvas.get_bounding_client_rect();
        let host = Rc::new_cyclic(|weak: &Weak<RefCell<Host>>| {
            let wake = weak.clone();
            let runtime = BrowserRuntime::new(move || {
                if let Some(host) = wake.upgrade() {
                    host.borrow().schedule();
                }
            });
            let mut ui = {
                let _entered = runtime.enter();
                let mut ui = Ui::with_fonts(
                    Size::new(rect.width() as f32, rect.height() as f32),
                    fonts,
                    root,
                );
                ui.set_clipboard(BrowserClipboard);
                ui.set_surfaces(surfaces.registrar());
                ui
            };
            ui.set_scale(window.device_pixel_ratio() as f32);
            let draw = weak.clone();
            RefCell::new(Host {
                window,
                canvas,
                input,
                surface,
                config,
                gpu,
                renderer,
                surfaces,
                ui: RefCell::new(ui),
                runtime,
                background,
                frame: Cell::new(None),
                composing: Cell::new(false),
                frames: Cell::new(0),
                callback: Closure::new(move |time| {
                    if let Some(host) = draw.upgrade() {
                        host.borrow_mut().draw(time);
                    }
                }),
            })
        });
        let resized = Rc::downgrade(&host);
        let resize = Closure::new(move || {
            if let Some(host) = resized.upgrade() {
                host.borrow().schedule();
            }
        });
        let observer = ResizeObserver::new(resize.as_ref().unchecked_ref())?;
        let mut app = Self {
            host,
            listeners: Vec::new(),
            observer,
            _resize: resize,
        };
        {
            let host = app.host.borrow();
            let body = document
                .body()
                .ok_or_else(|| JsValue::from_str("no browser body"))?;
            body.append_child(&host.input)?;
            app.observer.observe(&host.canvas);
        }
        app.install()?;
        app.host.borrow().schedule();
        Ok(app)
    }

    pub fn frames_presented(&self) -> u64 {
        self.host.borrow().frames.get()
    }

    fn listen(
        &mut self,
        target: &EventTarget,
        name: &'static str,
        handler: impl Fn(&Host, Event) + 'static,
    ) -> Result<(), BrowserError> {
        let host = Rc::downgrade(&self.host);
        let closure = Closure::new(move |event: Event| {
            if let Some(host) = host.upgrade() {
                let host = host.borrow();
                let _entered = host.runtime.enter();
                handler(&host, event);
                host.schedule();
            }
        });
        target.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())?;
        self.listeners.push(Listener(target.clone(), name, closure));
        Ok(())
    }

    fn install(&mut self) -> Result<(), BrowserError> {
        let (canvas, input, window) = {
            let host = self.host.borrow();
            (
                EventTarget::from(host.canvas.clone()),
                EventTarget::from(host.input.clone()),
                EventTarget::from(host.window.clone()),
            )
        };
        self.listen(&window, "resize", |_, _| {})?;
        let document = self.host.borrow().window.document();
        if let Some(document) = document {
            let document = EventTarget::from(document);
            self.listen(&document, "visibilitychange", |_, _| {})?;
        }
        self.listen(&window, "blur", |host, _| host.blur())?;
        for name in [
            "pointerdown",
            "pointermove",
            "pointerup",
            "pointercancel",
            "lostpointercapture",
        ] {
            self.listen(&canvas, name, move |host, event| {
                let Some(event) = event.dyn_ref::<PointerEvent>() else {
                    return;
                };
                let rect = host.canvas.get_bounding_client_rect();
                let mut ui = host.ui.borrow_mut();
                ui.set_modifiers(modifiers(
                    event.ctrl_key(),
                    event.alt_key(),
                    event.shift_key(),
                    event.meta_key(),
                ));
                if matches!(name, "pointercancel" | "lostpointercapture") {
                    ui.pointer_cancel();
                    return;
                }
                ui.pointer_moved(Point::new(
                    (f64::from(event.client_x()) - rect.left()) as f32,
                    (f64::from(event.client_y()) - rect.top()) as f32,
                ));
                let button = match event.button() {
                    0 => PointerButton::Primary,
                    1 => PointerButton::Middle,
                    2 => PointerButton::Secondary,
                    _ => return,
                };
                if name == "pointerdown" {
                    ui.pointer_down(button);
                    drop(ui);
                    let _ = host.canvas.set_pointer_capture(event.pointer_id());
                    host.sync_input(true);
                    event.prevent_default();
                } else if name == "pointerup" {
                    ui.pointer_up(button);
                }
            })?;
        }
        self.listen(&canvas, "pointerleave", |host, _| {
            host.ui.borrow_mut().pointer_left()
        })?;
        self.listen(&canvas, "wheel", |host, event| {
            let Some(event) = event.dyn_ref::<WheelEvent>() else {
                return;
            };
            if event.ctrl_key() {
                return;
            }
            let rect = host.canvas.get_bounding_client_rect();
            let scale = match event.delta_mode() {
                1 => 16.0,
                2 => rect.height(),
                _ => 1.0,
            };
            let mut ui = host.ui.borrow_mut();
            ui.pointer_moved(Point::new(
                (f64::from(event.client_x()) - rect.left()) as f32,
                (f64::from(event.client_y()) - rect.top()) as f32,
            ));
            if ui.scroll(Point::new(
                (-event.delta_x() * scale) as f32,
                (-event.delta_y() * scale) as f32,
            )) {
                event.prevent_default();
            }
        })?;
        for target in [&canvas, &input] {
            for name in ["keydown", "keyup"] {
                self.listen(target, name, move |host, event| {
                    let Some(event) = event.dyn_ref::<KeyboardEvent>() else {
                        return;
                    };
                    if event.is_composing() || host.composing.get() {
                        return;
                    }
                    let mut key = KeyEvent::new(dom_key(event)).with_modifiers(modifiers(
                        event.ctrl_key(),
                        event.alt_key(),
                        event.shift_key(),
                        event.meta_key(),
                    ));
                    let mut ui = host.ui.borrow_mut();
                    if ui.accepts_text()
                        && primary(event, &host.window)
                        && matches!(event.key().to_ascii_lowercase().as_str(), "x" | "c" | "v")
                    {
                        return;
                    }
                    key.repeat = event.repeat();
                    let handled = if name == "keydown" {
                        ui.key_down(key)
                    } else {
                        ui.key_up(key)
                    };
                    drop(ui);
                    if handled {
                        event.prevent_default();
                    }
                    host.sync_input(false);
                })?;
            }
        }
        self.listen(&input, "beforeinput", |host, event| {
            let Some(event) = event.dyn_ref::<InputEvent>() else {
                return;
            };
            if host.composing.get() || event.is_composing() {
                return;
            }
            if matches!(
                event.input_type().as_str(),
                "insertText" | "insertReplacementText"
            ) && let Some(data) = event.data()
                && event.cancelable()
            {
                host.ui.borrow_mut().ime(ImeEvent::Commit(data));
                event.prevent_default();
            }
        })?;
        self.listen(&input, "input", |host, event| {
            if host.composing.get() {
                return;
            }
            if event.dyn_ref::<InputEvent>().is_some_and(|event| {
                matches!(
                    event.input_type().as_str(),
                    "insertText" | "insertReplacementText"
                )
            }) {
                let text = host.input.value();
                if !text.is_empty() {
                    host.ui.borrow_mut().ime(ImeEvent::Commit(text));
                }
            }
            host.input.set_value("");
        })?;
        for name in ["compositionstart", "compositionupdate", "compositionend"] {
            self.listen(&input, name, move |host, event| {
                let Some(event) = event.dyn_ref::<CompositionEvent>() else {
                    return;
                };
                let text = event.data().unwrap_or_default();
                if name == "compositionstart" {
                    host.composing.set(true);
                }
                if !host.composing.get() {
                    return;
                }
                if name == "compositionend" {
                    host.composing.set(false);
                    if text.is_empty() {
                        host.ui
                            .borrow_mut()
                            .ime(ImeEvent::Preedit { text, cursor: None });
                    } else {
                        host.ui.borrow_mut().ime(ImeEvent::Commit(text));
                    }
                    host.input.set_value("");
                } else {
                    let end = text.len();
                    host.ui.borrow_mut().ime(ImeEvent::Preedit {
                        text,
                        cursor: Some((end, end)),
                    });
                }
            })?;
        }
        for name in ["copy", "cut", "paste"] {
            self.listen(&input, name, move |host, event| {
                let Some(event) = event.dyn_ref::<ClipboardEvent>() else {
                    return;
                };
                let Some(data) = event.clipboard_data() else {
                    return;
                };
                let mut ui = host.ui.borrow_mut();
                if name == "paste" {
                    if let Some(target) = ui.clipboard_target()
                        && let Ok(text) = data.get_data("text/plain")
                    {
                        event.prevent_default();
                        ui.paste_text(target, text);
                    }
                } else if let Some((target, text)) = ui.clipboard_selection()
                    && data.set_data("text/plain", &text).is_ok()
                {
                    event.prevent_default();
                    if name == "cut" {
                        ui.cut_selection(target);
                    }
                }
            })?;
        }
        Ok(())
    }
}

impl Drop for BrowserApp {
    fn drop(&mut self) {
        self.listeners.clear();
        self.observer.disconnect();
        let host = self.host.borrow();
        if let Some(frame) = host.frame.take() {
            let _ = host.window.cancel_animation_frame(frame);
        }
        host.input.remove();
    }
}

impl Host {
    fn schedule(&self) {
        if self.frame.get().is_none() && !self.window.document().is_some_and(|doc| doc.hidden()) {
            self.frame.set(
                self.window
                    .request_animation_frame(self.callback.as_ref().unchecked_ref())
                    .ok(),
            );
        }
    }

    fn blur(&self) {
        self.composing.set(false);
        self.input.set_value("");
        let mut ui = self.ui.borrow_mut();
        ui.pointer_cancel();
        ui.clear_focus();
    }

    fn sync_input(&self, focus: bool) {
        let mut ui = self.ui.borrow_mut();
        let editable = ui.accepts_text();
        let area = ui.text_cursor_area();
        drop(ui);
        let rect = self.canvas.get_bounding_client_rect();
        if let Some(area) = area.filter(|_| editable) {
            let _ = self.input.style().set_property(
                "left",
                &format!("{}px", rect.left() + f64::from(area.origin.x)),
            );
            let _ = self.input.style().set_property(
                "top",
                &format!(
                    "{}px",
                    rect.top() + f64::from(area.origin.y + area.size.height)
                ),
            );
        }
        let active = self.window.document().and_then(|doc| doc.active_element());
        if editable
            && (focus
                || active
                    .as_ref()
                    .is_some_and(|node| node == self.canvas.as_ref()))
        {
            let _ = self.input.focus();
        } else if !editable
            && (focus
                || active
                    .as_ref()
                    .is_some_and(|node| node == self.input.as_ref()))
        {
            let _ = self.canvas.focus();
        }
    }

    fn draw(&mut self, time: f64) {
        self.frame.set(None);
        let _entered = self.runtime.enter();
        let rect = self.canvas.get_bounding_client_rect();
        let scale = self.window.device_pixel_ratio();
        let max = f64::from(self.gpu.device.limits().max_texture_dimension_2d);
        let width = (rect.width() * scale).round().min(max) as u32;
        let height = (rect.height() * scale).round().min(max) as u32;
        if width == 0 || height == 0 {
            return;
        }
        let resized = self.config.width != width
            || self.config.height != height
            || self.frames.get() == 0
            || self.ui.get_mut().scale() != scale as f32;
        if resized {
            if self.canvas.width() != width {
                self.canvas.set_width(width);
            }
            if self.canvas.height() != height {
                self.canvas.set_height(height);
            }
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.gpu.device, &self.config);
            self.ui
                .get_mut()
                .set_size(Size::new(rect.width() as f32, rect.height() as f32));
            self.ui.get_mut().set_scale(scale as f32);
        }
        let ui = self.ui.get_mut();
        for command in ui.take_window_commands() {
            match command {
                WindowCommand::Menu(item) => {
                    item.dispatch(ui);
                }
                WindowCommand::SetTitle(title) => {
                    if let Some(doc) = self.window.document() {
                        doc.set_title(&title);
                    }
                }
                _ => {}
            }
        }
        let changed = ui.update_at(time / 1000.0);
        let cursor = match ui.cursor() {
            Cursor::Default => "default",
            Cursor::Pointer => "pointer",
            Cursor::Text => "text",
            Cursor::Grab => "grab",
            Cursor::Grabbing => "grabbing",
            Cursor::NotAllowed => "not-allowed",
            Cursor::Crosshair => "crosshair",
            Cursor::ResizeHorizontal => "ew-resize",
            Cursor::ResizeVertical => "ns-resize",
        };
        let _ = self.canvas.style().set_property("cursor", cursor);
        if changed || resized {
            match self.surface.get_current_texture() {
                CurrentSurfaceTexture::Success(frame)
                | CurrentSurfaceTexture::Suboptimal(frame) => {
                    let view = frame.texture.create_view(&TextureViewDescriptor::default());
                    let viewport = Viewport::new(width, height, ui.scale());
                    self.surfaces.latch(Instant::now());
                    let frame_ui = ui.frame();
                    self.renderer.render(
                        &view,
                        viewport,
                        self.background,
                        &frame_ui.scene,
                        frame_ui.atlas,
                        &self.surfaces,
                    );
                    self.surfaces.maintain();
                    self.gpu.queue.present(frame);
                    self.frames.set(self.frames.get() + 1);
                }
                CurrentSurfaceTexture::Outdated
                | CurrentSurfaceTexture::Lost
                | CurrentSurfaceTexture::Timeout => {
                    self.surface.configure(&self.gpu.device, &self.config);
                    self.frames.set(0);
                    self.schedule();
                }
                _ => {}
            }
        }
        if self.ui.get_mut().needs_frame() {
            self.schedule();
        }
        self.sync_input(false);
    }
}

fn modifiers(ctrl: bool, alt: bool, shift: bool, meta: bool) -> Modifiers {
    Modifiers {
        ctrl,
        alt,
        shift,
        meta,
    }
}

fn primary(event: &KeyboardEvent, window: &Window) -> bool {
    let mac = window
        .navigator()
        .platform()
        .is_ok_and(|name| name.starts_with("Mac"));
    if mac {
        event.meta_key()
    } else {
        event.ctrl_key()
    }
}

fn dom_key(event: &KeyboardEvent) -> Key {
    let key = event.key();
    match key.as_str() {
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Tab" => Key::Tab,
        " " => Key::Space,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "?" if event.shift_key() => Key::character("/"),
        _ if key.chars().count() == 1 => Key::character(&key),
        _ => key
            .strip_prefix('F')
            .and_then(|number| number.parse().ok())
            .map_or(Key::Other, Key::F),
    }
}
