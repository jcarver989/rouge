use super::clipboard::WebClipboard;
use super::dom_key;
use super::runtime::BrowserRuntime;
use super::trace::PerformanceLayer;
use crate::app::{App, AppError, RootFn, WindowOptions};
use crate::driver::{Drawn, Driver, TextInput, connect, latch};
use crate::platform::{LINE_SCROLL, css_cursor, window_root};
use crate::profile::{ProfileSettings, subscribe};
use js_sys::{Array, Error as JsError, Function, Reflect};
use rouge_core::{
    Appearance, EditAction, ImeEvent, KeyEvent, Menu, Modifiers, Point, PointerButton, Size,
    TouchPhase, Ui, WindowCommand,
};
use rouge_render::{Gpu, Surfaces};
use std::borrow::Cow;
use std::cell::{Cell, OnceCell, RefCell};
use std::mem::replace;
use std::rc::{Rc, Weak};
use std::time::Duration;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{
    AddEventListenerOptions, ClipboardEvent, CompositionEvent, Document, DomRect, Element, Event,
    EventTarget, HtmlCanvasElement, HtmlElement, HtmlTextAreaElement, InputEvent, KeyboardEvent,
    MediaQueryList, PointerEvent, ResizeObserver, ResizeObserverBoxOptions, ResizeObserverEntry,
    ResizeObserverOptions, ResizeObserverSize, UrlSearchParams, WheelEvent, Window, console,
};
use wgpu::{Instance, SurfaceTarget};

const CANVAS_STYLE: &str = "display:block;touch-action:none;outline:none;\
    -webkit-tap-highlight-color:transparent;user-select:none;-webkit-user-select:none;";
const PAGE_STYLE: &str = "position:fixed;inset:0;width:100%;height:100%;";
const INPUT_STYLE: &str = "position:fixed;left:0;top:0;opacity:0;pointer-events:none;\
    width:1px;height:1px;padding:0;border:0;resize:none;font-size:16px;";

type Slot = Rc<OnceCell<Weak<Host>>>;
type Handler = Rc<dyn Fn(&Host, &mut State, &str, &Event)>;

thread_local! {
    static WINDOWS: RefCell<Vec<Rc<Host>>> = const { RefCell::new(Vec::new()) };
    static GPU: RefCell<Option<(Instance, Gpu)>> = const { RefCell::new(None) };
}

impl From<JsValue> for AppError {
    fn from(value: JsValue) -> Self {
        AppError::Browser(
            value
                .as_string()
                .or_else(|| {
                    value
                        .dyn_ref::<JsError>()
                        .map(|error| String::from(error.message()))
                })
                .unwrap_or_else(|| format!("{value:?}")),
        )
    }
}

pub(crate) fn run(mut app: App) -> Result<(), AppError> {
    if !app.menus.is_empty() && !app.menus.iter().any(|menu| menu.title == "Edit") {
        app.menus.insert(app.menus.len().min(1), Menu::edit());
    }
    let params = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| UrlSearchParams::new_with_str(&search).ok());
    let settings = ProfileSettings::read(|name| params.as_ref()?.get(name));
    if settings.trace.is_some() {
        subscribe(PerformanceLayer);
    }
    spawn_local(async move {
        for (options, root) in app.windows {
            let canvas = options.canvas.clone();
            match Host::mount(options, root, &app.menus, &app.fonts, &settings).await {
                Ok(host) => WINDOWS.with_borrow_mut(|windows| windows.push(host)),
                Err(error) => report(canvas.as_deref(), &error),
            }
        }
    });
    Ok(())
}

fn report(canvas: Option<&str>, error: &AppError) {
    console::error_1(&format!("rouge: {error}").into());
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Ok(message) = document.create_element("p") else {
        return;
    };
    message.set_text_content(Some(&format!("This app could not start: {error}")));
    let _ = message.set_attribute("role", "alert");
    let parent = canvas
        .and_then(|id| document.get_element_by_id(id))
        .and_then(|element| element.parent_element())
        .or_else(|| document.body().map(Element::from));
    if let Some(parent) = parent {
        let _ = parent.append_child(&message);
    }
}

struct Listener(EventTarget, &'static str, Closure<dyn FnMut(Event)>);

impl Listener {
    fn new(
        slot: &Slot,
        target: EventTarget,
        name: &'static str,
        handler: Handler,
    ) -> Result<Listener, AppError> {
        let closure: Closure<dyn FnMut(Event)> =
            Closure::new(bind(slot, move |host, event: Event| {
                host.update(|host, state| handler(host, state, name, &event));
            }));
        let options = AddEventListenerOptions::new();
        options.set_passive(false);
        target.add_event_listener_with_callback_and_add_event_listener_options(
            name,
            closure.as_ref().unchecked_ref(),
            &options,
        )?;
        Ok(Listener(target, name, closure))
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self
            .0
            .remove_event_listener_with_callback(self.1, self.2.as_ref().unchecked_ref());
    }
}

struct State {
    driver: Driver,
    surfaces: Surfaces,
    pressed: bool,
    composing: bool,
    touch: Option<i32>,
    last_frame: Option<f64>,
    frame_ms: f64,
}

impl State {
    fn tick(&mut self, time: f64) -> Duration {
        if let Some(last) = self.last_frame.replace(time) {
            let delta = time - last;
            if (4.0..50.0).contains(&delta) {
                self.frame_ms += (delta - self.frame_ms) * 0.1;
            }
        }
        Duration::from_secs_f64(self.frame_ms / 1000.0)
    }
}

struct Host {
    window: Window,
    document: Document,
    canvas: HtmlCanvasElement,
    input: HtmlTextAreaElement,
    mac: bool,
    state: RefCell<State>,
    runtime: BrowserRuntime,
    clipboard: WebClipboard,
    frame: Cell<Option<i32>>,
    rect: RefCell<Option<DomRect>>,
    pixels: Cell<(u32, u32)>,
    resized: Cell<bool>,
    editable: Cell<bool>,
    animate: Closure<dyn FnMut(f64)>,
    observer: ResizeObserver,
    _measured: Closure<dyn FnMut(Array)>,
    _listeners: Vec<Listener>,
}

impl Drop for Host {
    fn drop(&mut self) {
        self.observer.disconnect();
        if let Some(frame) = self.frame.take() {
            let _ = self.window.cancel_animation_frame(frame);
        }
        self.input.remove();
    }
}

impl Host {
    async fn mount(
        options: WindowOptions,
        root: RootFn,
        menus: &[Menu],
        fonts: &[Cow<'static, [u8]>],
        settings: &ProfileSettings,
    ) -> Result<Rc<Host>, AppError> {
        let window = web_sys::window().ok_or_else(|| missing("window"))?;
        let document = window.document().ok_or_else(|| missing("document"))?;
        let canvas = canvas(&document, options.canvas.as_deref())?;
        let target = SurfaceTarget::Canvas(canvas.clone());
        let mut shared = GPU.take();
        let connected = connect(&mut shared, Instance::default, target).await;
        GPU.set(shared);
        let (gpu, surface) = connected?;
        let input = textarea(&document)?;
        canvas.set_tab_index(0);
        let rect = canvas.get_bounding_client_rect();
        let scale = window.device_pixel_ratio();
        let pixels = physical(rect.width(), rect.height(), scale);
        let clipboard = WebClipboard::default();
        let slot = Slot::default();
        let wake = bind(&slot, |host, ()| host.update(|_, _| {}));
        let runtime = BrowserRuntime::new(move || wake(()));
        let surfaces = Surfaces::new(&gpu).with_waker(runtime.waker());
        let driver = {
            let _entered = runtime.enter();
            let (title, shortcuts) = (options.title, menus.to_vec());
            let logical = Size::new(rect.width() as f32, rect.height() as f32);
            let mut ui = Ui::with_fonts(logical, fonts.iter().cloned(), move || {
                window_root(title, shortcuts, false, root)
            });
            ui.set_device_scale(scale as f32);
            ui.set_appearance(appearance(&window));
            ui.set_clipboard(clipboard.clone());
            ui.bind_menus(menus);
            Driver::new(gpu, surface, pixels, ui, options.background, None, settings)?
        };
        let animate = Closure::new(bind(&slot, Host::draw));
        let measured: Closure<dyn FnMut(Array)> =
            Closure::new(bind(&slot, |host, entries: Array| host.observe(&entries)));
        let observer = ResizeObserver::new(measured.as_ref().unchecked_ref())?;
        let listeners = listeners(&slot, &window, &document, &canvas, &input)?;
        let host = Rc::new(Host {
            mac: is_mac(&window),
            window,
            document,
            canvas,
            input,
            state: RefCell::new(State {
                driver,
                surfaces,
                pressed: false,
                composing: false,
                touch: None,
                last_frame: None,
                frame_ms: 1000.0 / 60.0,
            }),
            runtime,
            clipboard,
            frame: Cell::new(None),
            rect: RefCell::new(Some(rect)),
            pixels: Cell::new(pixels),
            resized: Cell::new(true),
            editable: Cell::new(false),
            animate,
            observer,
            _measured: measured,
            _listeners: listeners,
        });
        let _ = slot.set(Rc::downgrade(&host));
        let options = ResizeObserverOptions::new();
        options.set_box(ResizeObserverBoxOptions::DevicePixelContentBox);
        let observe: Function = field(&host.observer, "observe").dyn_into()?;
        if observe
            .call2(&host.observer, &host.canvas, &options)
            .is_err()
        {
            host.observer.observe(&host.canvas);
        }
        host.update(|_, _| {});
        Ok(host)
    }

    fn update(&self, change: impl FnOnce(&Host, &mut State)) {
        let _entered = self.runtime.enter();
        let Ok(mut state) = self.state.try_borrow_mut() else {
            return;
        };
        change(self, &mut state);
        self.commands(&mut state);
        let wanted = self.wanted(&mut state);
        drop(state);
        if wanted {
            self.schedule();
        }
    }

    fn schedule(&self) {
        if self.frame.get().is_none() && !self.document.hidden() {
            let callback = self.animate.as_ref().unchecked_ref();
            self.frame
                .set(self.window.request_animation_frame(callback).ok());
        }
    }

    fn commands(&self, state: &mut State) {
        for command in state.driver.commands() {
            match command {
                WindowCommand::SetTitle(title) => self.document.set_title(&title),
                WindowCommand::Focus => {
                    let _ = self.canvas.focus();
                }
                _ => {}
            }
        }
    }

    fn wanted(&self, state: &mut State) -> bool {
        self.resized.get() || {
            let surfaced = latch(&mut state.surfaces, [&mut state.driver]);
            state.driver.wants_frame(surfaced)
        }
    }

    fn invalidate_rect(&self) {
        self.rect.replace(None);
    }

    fn rect(&self) -> DomRect {
        self.rect
            .borrow_mut()
            .get_or_insert_with(|| self.canvas.get_bounding_client_rect())
            .clone()
    }

    fn zoom(&self, ui: &Ui) -> f64 {
        f64::from(ui.scale()) / self.window.device_pixel_ratio()
    }

    fn local(&self, event: &JsValue, ui: &Ui) -> Point {
        let rect = self.rect();
        let zoom = self.zoom(ui);
        let axis = |name: &str, origin: f64| {
            ((field(event, name).as_f64().unwrap_or_default() - origin) / zoom) as f32
        };
        Point::new(axis("clientX", rect.left()), axis("clientY", rect.top()))
    }

    fn observe(&self, entries: &Array) {
        self.invalidate_rect();
        let Some(entry) = entries
            .iter()
            .last()
            .and_then(|entry| entry.dyn_into::<ResizeObserverEntry>().ok())
        else {
            return;
        };
        let exact = field(&entry, "devicePixelContentBoxSize")
            .dyn_into::<Array>()
            .ok()
            .and_then(|sizes| sizes.get(0).dyn_into::<ResizeObserverSize>().ok());
        let rect = entry.content_rect();
        self.pixels.set(match exact {
            Some(size) => physical(size.inline_size(), size.block_size(), 1.0),
            None => physical(
                rect.width(),
                rect.height(),
                self.window.device_pixel_ratio(),
            ),
        });
        self.resized.set(true);
        self.schedule();
    }

    fn draw(&self, time: f64) {
        self.frame.set(None);
        let _entered = self.runtime.enter();
        let Ok(mut state) = self.state.try_borrow_mut() else {
            self.schedule();
            return;
        };
        let interval = state.tick(time);
        if self.resized.take() {
            let max = state.driver.max_dimension();
            let (width, height) = self.pixels.get();
            let (width, height) = (width.min(max), height.min(max));
            if width > 0 && height > 0 {
                if self.canvas.width() != width {
                    self.canvas.set_width(width);
                }
                if self.canvas.height() != height {
                    self.canvas.set_height(height);
                }
                let scale = self.window.device_pixel_ratio() as f32;
                state.driver.resize((width, height), scale);
            }
        }
        self.commands(&mut state);
        let State {
            driver, surfaces, ..
        } = &mut *state;
        if driver.draw(surfaces, interval, || {}) == Drawn::Retry {
            self.resized.set(true);
        }
        if driver.profile_finished()
            && let Some(summary) = driver.profile_summary()
        {
            console::log_1(&summary.into());
        }
        let wanted = self.wanted(&mut state);
        let input = self.sync(&mut state);
        drop(state);
        self.place_input(input, false);
        if wanted {
            self.schedule();
        }
    }

    fn sync(&self, state: &mut State) -> Option<TextInput> {
        let changes = state.driver.changes();
        if let Some(cursor) = changes.cursor {
            let _ = self
                .canvas
                .style()
                .set_property("cursor", css_cursor(cursor));
        }
        changes.text_input
    }

    fn place_input(&self, input: Option<TextInput>, focus: bool) {
        if let Some(input) = input {
            self.editable.set(matches!(input, TextInput::On { .. }));
            if let TextInput::On { area: Some(area) } = input {
                let rect = self.rect();
                let ratio = self.window.device_pixel_ratio();
                let _ = self.input.style().set_property(
                    "transform",
                    &format!(
                        "translate({}px,{}px)",
                        rect.left() + f64::from(area.origin.x) / ratio,
                        rect.top() + f64::from(area.origin.y + area.size.height) / ratio
                    ),
                );
            }
        }
        let (from, to): (&HtmlElement, &HtmlElement) = if self.editable.get() {
            (&self.canvas, &self.input)
        } else {
            (&self.input, &self.canvas)
        };
        let active = self.document.active_element();
        if focus || active.is_some_and(|active| &active == from.as_ref()) {
            let _ = to.focus();
        }
    }

    fn moved(&self, _: &mut State, _: &str, _: &Event) {
        self.invalidate_rect();
    }

    fn rescale(&self, _: &mut State, _: &str, _: &Event) {
        self.invalidate_rect();
        self.resized.set(true);
    }

    fn scheme(&self, state: &mut State, _: &str, _: &Event) {
        state.driver.ui().set_appearance(appearance(&self.window));
    }

    fn focus(_: &Self, state: &mut State, _: &str, _: &Event) {
        state.driver.ui().set_focused(true);
    }

    fn blur(&self, state: &mut State, _: &str, _: &Event) {
        state.composing = false;
        state.touch = None;
        self.input.set_value("");
        let ui = state.driver.ui();
        ui.set_focused(false);
        ui.pointer_cancel();
        ui.clear_focus();
    }

    fn visibility(&self, state: &mut State, _: &str, _: &Event) {
        let visible = !self.document.hidden();
        state.driver.ui().set_visible(visible);
        if visible {
            self.resized.set(true);
        }
    }

    fn context_menu(_: &Self, _: &mut State, _: &str, event: &Event) {
        event.prevent_default();
    }

    fn leave(_: &Self, state: &mut State, _: &str, event: &Event) {
        let touch = event
            .dyn_ref::<PointerEvent>()
            .is_some_and(|event| event.pointer_type() == "touch");
        if !touch && state.touch.is_none() {
            state.driver.ui().pointer_left();
        }
    }

    fn pointer(&self, state: &mut State, name: &str, event: &PointerEvent) {
        let ui = state.driver.ui();
        let position = self.local(event, ui);
        ui.set_modifiers(modifiers(event));
        if event.pointer_type() == "touch" {
            self.touch(state, name, event, position);
            return;
        }
        if matches!(name, "pointercancel" | "lostpointercapture") {
            if replace(&mut state.pressed, false) || name == "pointercancel" {
                ui.pointer_cancel();
            }
            return;
        }
        ui.pointer_moved(position);
        let button = match event.button() {
            0 => PointerButton::Primary,
            1 => PointerButton::Middle,
            2 => PointerButton::Secondary,
            -1 => return,
            other => PointerButton::Other(other as u16),
        };
        match name {
            "pointerdown" => {
                state.pressed = true;
                ui.pointer_down(button);
                let _ = self.canvas.set_pointer_capture(event.pointer_id());
                self.place_input(self.sync(state), true);
                event.prevent_default();
            }
            "pointerup" => {
                state.pressed = false;
                ui.pointer_up(button);
            }
            _ => {}
        }
    }

    fn touch(&self, state: &mut State, name: &str, event: &PointerEvent, position: Point) {
        if !event.is_primary() {
            return;
        }
        let phase = match name {
            "pointerdown" => TouchPhase::Start,
            "pointermove" if state.touch.is_some() => TouchPhase::Move,
            "pointerup" => TouchPhase::End,
            "pointercancel" | "lostpointercapture" => TouchPhase::Cancel,
            _ => return,
        };
        if phase == TouchPhase::Start {
            state.touch = Some(event.pointer_id());
            let _ = self.canvas.set_pointer_capture(event.pointer_id());
        } else if matches!(phase, TouchPhase::End | TouchPhase::Cancel)
            && state.touch.take().is_none()
        {
            return;
        }
        state.driver.ui().touch(phase, position);
        event.prevent_default();
        if phase == TouchPhase::End {
            self.place_input(self.sync(state), true);
        }
    }

    fn wheel(&self, state: &mut State, _: &str, event: &WheelEvent) {
        if event.ctrl_key() {
            return;
        }
        let unit = match event.delta_mode() {
            WheelEvent::DOM_DELTA_LINE => f64::from(LINE_SCROLL),
            WheelEvent::DOM_DELTA_PAGE => self.rect().height(),
            _ => 1.0,
        };
        let ui = state.driver.ui();
        let position = self.local(event, ui);
        let unit = unit / self.zoom(ui);
        ui.pointer_moved(position);
        if ui.scroll(Point::new(
            (-event.delta_x() * unit) as f32,
            (-event.delta_y() * unit) as f32,
        )) {
            event.prevent_default();
        }
    }

    fn key(&self, state: &mut State, name: &str, event: &KeyboardEvent) {
        if event.is_composing() || state.composing {
            return;
        }
        let ui = state.driver.ui();
        let primary = if self.mac {
            event.meta_key()
        } else {
            event.ctrl_key()
        };
        if primary
            && ui.accepts_text()
            && matches!(event.key().to_ascii_lowercase().as_str(), "x" | "c" | "v")
        {
            return;
        }
        let mut key = KeyEvent::new(dom_key(&event.key(), &event.code(), event.shift_key()))
            .with_modifiers(modifiers(event));
        key.repeat = event.repeat();
        let handled = if name == "keydown" {
            ui.key_down(key)
        } else {
            ui.key_up(key)
        };
        if handled {
            event.prevent_default();
        }
        self.place_input(self.sync(state), false);
    }

    fn before_input(_: &Self, state: &mut State, _: &str, event: &InputEvent) {
        if state.composing || event.is_composing() || !inserts(event) || !event.cancelable() {
            return;
        }
        if let Some(data) = event.data() {
            state.driver.ui().ime(ImeEvent::Commit(data));
            event.prevent_default();
        }
    }

    fn inserted(&self, state: &mut State, _: &str, event: &Event) {
        if state.composing {
            return;
        }
        if event.dyn_ref::<InputEvent>().is_some_and(inserts) {
            let text = self.input.value();
            if !text.is_empty() {
                state.driver.ui().ime(ImeEvent::Commit(text));
            }
        }
        self.input.set_value("");
    }

    fn compose(&self, state: &mut State, name: &str, event: &CompositionEvent) {
        let text = event.data().unwrap_or_default();
        state.composing |= name == "compositionstart";
        if !state.composing {
            return;
        }
        let ui = state.driver.ui();
        if name == "compositionend" {
            state.composing = false;
            ui.ime(if text.is_empty() {
                ImeEvent::Preedit { text, cursor: None }
            } else {
                ImeEvent::Commit(text)
            });
            self.input.set_value("");
        } else {
            let end = text.len();
            ui.ime(ImeEvent::Preedit {
                text,
                cursor: Some((end, end)),
            });
        }
    }

    fn clipboard_event(&self, state: &mut State, name: &str, event: &ClipboardEvent) {
        let Some(data) = event.clipboard_data() else {
            return;
        };
        let ui = state.driver.ui();
        let handled = if name == "paste" {
            let Ok(text) = data.get_data("text/plain") else {
                return;
            };
            self.clipboard
                .paste_from(text, || ui.dispatch_action(EditAction::Paste))
        } else {
            let action = if name == "cut" {
                EditAction::Cut
            } else {
                EditAction::Copy
            };
            self.clipboard
                .copy_into(|| ui.dispatch_action(action))
                .is_some_and(|text| data.set_data("text/plain", &text).is_ok())
        };
        if handled {
            event.prevent_default();
        }
    }
}

fn bind<T: 'static>(slot: &Slot, action: impl Fn(&Host, T) + 'static) -> impl Fn(T) + 'static {
    let slot = slot.clone();
    move |value| {
        if let Some(host) = slot.get().and_then(Weak::upgrade) {
            action(&host, value);
        }
    }
}

fn on<T: JsCast + 'static>(handler: impl Fn(&Host, &mut State, &str, &T) + 'static) -> Handler {
    Rc::new(
        move |host: &Host, state: &mut State, name: &str, event: &Event| {
            if let Some(event) = event.dyn_ref::<T>() {
                handler(host, state, name, event);
            }
        },
    )
}

fn listeners(
    slot: &Slot,
    window: &Window,
    document: &Document,
    canvas: &HtmlCanvasElement,
    input: &HtmlTextAreaElement,
) -> Result<Vec<Listener>, AppError> {
    let page = EventTarget::from(window.clone());
    let document = EventTarget::from(document.clone());
    let canvas = EventTarget::from(canvas.clone());
    let input = EventTarget::from(input.clone());
    let resolution = format!("(resolution: {}dppx)", window.device_pixel_ratio());
    let resolution = window.match_media(&resolution)?;
    let mut table = vec![
        (page.clone(), "resize scroll", on(Host::moved)),
        (page.clone(), "focus", on(Host::focus)),
        (page, "blur", on(Host::blur)),
        (document, "visibilitychange", on(Host::visibility)),
        (
            canvas.clone(),
            "pointerdown pointermove pointerup pointercancel lostpointercapture",
            on(Host::pointer),
        ),
        (canvas.clone(), "pointerleave", on(Host::leave)),
        (canvas.clone(), "contextmenu", on(Host::context_menu)),
        (canvas.clone(), "wheel", on(Host::wheel)),
        (canvas.clone(), "keydown keyup", on(Host::key)),
        (canvas, "copy cut paste", on(Host::clipboard_event)),
        (input.clone(), "keydown keyup", on(Host::key)),
        (input.clone(), "copy cut paste", on(Host::clipboard_event)),
        (input.clone(), "beforeinput", on(Host::before_input)),
        (input.clone(), "input", on(Host::inserted)),
        (
            input,
            "compositionstart compositionupdate compositionend",
            on(Host::compose),
        ),
    ];
    table.extend(color_scheme(window).map(|query| (query.into(), "change", on(Host::scheme))));
    table.extend(resolution.map(|query| (query.into(), "change", on(Host::rescale))));
    table
        .into_iter()
        .flat_map(|(target, names, handler)| {
            names
                .split(' ')
                .map(move |name| Listener::new(slot, target.clone(), name, handler.clone()))
        })
        .collect()
}

fn canvas(document: &Document, id: Option<&str>) -> Result<HtmlCanvasElement, AppError> {
    let parent = id
        .map(|id| {
            document
                .get_element_by_id(id)
                .ok_or_else(|| AppError::Browser(format!("no element with id `{id}`")))
        })
        .transpose()?;
    if let Some(canvas) = parent
        .as_ref()
        .and_then(|element| element.dyn_ref::<HtmlCanvasElement>())
    {
        let style = canvas.style();
        style.set_css_text(&format!("{CANVAS_STYLE}{}", style.css_text()));
        return Ok(canvas.clone());
    }
    let canvas = document
        .create_element("canvas")?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(JsValue::from)?;
    let (parent, size) = match parent {
        Some(parent) => (parent, "width:100%;height:100%;"),
        None => (body(document)?.into(), PAGE_STYLE),
    };
    canvas.set_attribute("style", &format!("{CANVAS_STYLE}{size}"))?;
    parent.append_child(&canvas)?;
    Ok(canvas)
}

fn textarea(document: &Document) -> Result<HtmlTextAreaElement, AppError> {
    let input = document
        .create_element("textarea")?
        .dyn_into::<HtmlTextAreaElement>()
        .map_err(JsValue::from)?;
    for (name, value) in [
        ("aria-label", "Text input"),
        ("autocapitalize", "off"),
        ("autocomplete", "off"),
        ("autocorrect", "off"),
        ("spellcheck", "false"),
        ("style", INPUT_STYLE),
    ] {
        input.set_attribute(name, value)?;
    }
    input.set_tab_index(-1);
    body(document)?.append_child(&input)?;
    Ok(input)
}

fn body(document: &Document) -> Result<HtmlElement, AppError> {
    document.body().ok_or_else(|| missing("body"))
}

fn missing(what: &str) -> AppError {
    AppError::Browser(format!("no browser {what}"))
}

fn physical(width: f64, height: f64, scale: f64) -> (u32, u32) {
    (
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    )
}

fn field(target: &JsValue, name: &str) -> JsValue {
    Reflect::get(target, &name.into()).unwrap_or_default()
}

fn inserts(event: &InputEvent) -> bool {
    matches!(
        event.input_type().as_str(),
        "insertText" | "insertReplacementText"
    )
}

fn color_scheme(window: &Window) -> Option<MediaQueryList> {
    window
        .match_media("(prefers-color-scheme: dark)")
        .ok()
        .flatten()
}

fn appearance(window: &Window) -> Appearance {
    if color_scheme(window).is_some_and(|query| query.matches()) {
        Appearance::Dark
    } else {
        Appearance::Light
    }
}

fn is_mac(window: &Window) -> bool {
    let navigator = window.navigator();
    let platform = field(&field(&navigator, "userAgentData"), "platform")
        .as_string()
        .or_else(|| navigator.platform().ok())
        .unwrap_or_default();
    platform.starts_with("Mac") || platform == "macOS" || platform.starts_with("iP")
}

fn modifiers(event: &JsValue) -> Modifiers {
    let flag = |name: &str| field(event, name).as_bool().unwrap_or_default();
    Modifiers {
        ctrl: flag("ctrlKey"),
        alt: flag("altKey"),
        shift: flag("shiftKey"),
        meta: flag("metaKey"),
    }
}
