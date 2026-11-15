use super::clipboard::WebClipboard;
use super::dom_key;
use super::trace::PerformanceLayer;
use crate::app::{App, AppError, RootFn, WindowOptions};
use crate::driver::{Driver, Setup, TextInput, connect, latch};
use crate::platform::{LINE_SCROLL, css_cursor, default_menus, window_root};
use crate::profile::{ProfileSettings, subscribe};
use js_sys::{Array, Error as JsError, Function, Reflect};
use rouge_core::{
    Appearance, EditAction, ImeEvent, KeyEvent, Menu, Modifiers, Point, PointerButton, TouchPhase,
    Ui, WindowCommand,
};
use rouge_reactive::Runtime;
use rouge_render::{Gpu, Surfaces};
use std::borrow::Cow;
use std::cell::{Cell, OnceCell, RefCell};
use std::mem::{forget, replace, take};
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
use web_time::Instant;
use wgpu::{Instance, SurfaceTarget};

const CANVAS_STYLE: &str = "display:block;touch-action:none;outline:none;\
    -webkit-tap-highlight-color:transparent;user-select:none;-webkit-user-select:none;";
const PAGE_STYLE: &str = "position:fixed;inset:0;width:100%;height:100%;";
const INPUT_STYLE: &str = "position:fixed;left:0;top:0;opacity:0;pointer-events:none;\
    width:1px;height:1px;padding:0;border:0;resize:none;font-size:16px;";

type Slot = Rc<OnceCell<Weak<Host>>>;

thread_local! {
    static HOSTS: RefCell<Vec<(u64, Weak<Host>)>> = const { RefCell::new(Vec::new()) };
    static NEXT_HOST: Cell<u64> = const { Cell::new(0) };
    static GPU: RefCell<Option<(Instance, Gpu)>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy)]
enum Source {
    Page,
    Document,
    Canvas,
    Input,
    Scheme,
    Resolution,
}

const PAGE_EVENTS: &[&str] = &["resize", "scroll", "focus", "blur"];
const DOCUMENT_EVENTS: &[&str] = &["visibilitychange"];
const CANVAS_EVENTS: &[&str] = &[
    "pointerdown",
    "pointermove",
    "pointerup",
    "pointercancel",
    "lostpointercapture",
    "pointerleave",
    "contextmenu",
    "wheel",
    "keydown",
    "keyup",
    "copy",
    "cut",
    "paste",
];
const INPUT_EVENTS: &[&str] = &[
    "keydown",
    "keyup",
    "copy",
    "cut",
    "paste",
    "beforeinput",
    "input",
    "compositionstart",
    "compositionupdate",
    "compositionend",
];
const MEDIA_EVENTS: &[&str] = &["change"];

fn host(id: u64) -> Option<Rc<Host>> {
    HOSTS.with_borrow(|hosts| {
        hosts
            .iter()
            .find(|(host, _)| *host == id)
            .and_then(|(_, host)| host.upgrade())
    })
}

fn wake(id: u64) {
    let Some(waking) = host(id).map(|host| host.waking.replace(true)) else {
        return;
    };
    if waking {
        return;
    }
    spawn_local(async move {
        if let Some(host) = host(id) {
            host.waking.set(false);
            host.update(|_, _| {});
        }
    });
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

pub struct Mounted {
    _hosts: Vec<Rc<Host>>,
}

pub(crate) fn run(app: App) -> Result<(), AppError> {
    spawn_local(async move {
        match mount_windows(app).await {
            Ok(hosts) => forget(hosts),
            Err((canvas, error)) => report(canvas.as_deref(), &error),
        }
    });
    Ok(())
}

pub(crate) async fn mount(app: App) -> Result<Mounted, AppError> {
    mount_windows(app)
        .await
        .map(|hosts| Mounted { _hosts: hosts })
        .map_err(|(_, error)| error)
}

async fn mount_windows(app: App) -> Result<Vec<Rc<Host>>, (Option<String>, AppError)> {
    let bar = !app.menus.is_empty();
    let menus = default_menus(app.menus);
    let params = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| UrlSearchParams::new_with_str(&search).ok());
    let settings = ProfileSettings::read(|name| params.as_ref()?.get(name));
    if settings.trace.is_some() {
        subscribe(PerformanceLayer);
    }
    let mut hosts = Vec::with_capacity(app.windows.len());
    for (options, root) in app.windows {
        let canvas = options.canvas.clone();
        let host = Host::mount(options, root, &menus, bar, &app.fonts, &settings).await;
        hosts.push(host.map_err(|error| (canvas, error))?);
    }
    Ok(hosts)
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

struct Listener {
    target: EventTarget,
    names: &'static [&'static str],
    closure: Closure<dyn FnMut(Event)>,
}

impl Listener {
    fn new(
        slot: &Slot,
        target: EventTarget,
        source: Source,
        names: &'static [&'static str],
    ) -> Result<Listener, AppError> {
        let closure: Closure<dyn FnMut(Event)> =
            Closure::new(bind(slot, move |host, event: Event| {
                host.update(|host, state| host.event(state, source, &event));
            }));
        let options = AddEventListenerOptions::new();
        options.set_passive(false);
        for name in names {
            target.add_event_listener_with_callback_and_add_event_listener_options(
                name,
                closure.as_ref().unchecked_ref(),
                &options,
            )?;
        }
        Ok(Listener {
            target,
            names,
            closure,
        })
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        for name in self.names {
            let _ = self
                .target
                .remove_event_listener_with_callback(name, self.closure.as_ref().unchecked_ref());
        }
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
    commands: Vec<WindowCommand>,
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
    id: u64,
    waking: Cell<bool>,
    runtime: Runtime,
    clipboard: WebClipboard,
    frame: Cell<Option<i32>>,
    timer: Cell<Option<i32>>,
    wake: Closure<dyn FnMut()>,
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
        if let Some(timer) = self.timer.take() {
            self.window.clear_timeout_with_handle(timer);
        }
        self.input.remove();
        HOSTS.with_borrow_mut(|hosts| hosts.retain(|(host, _)| *host != self.id));
    }
}

impl Host {
    async fn mount(
        options: WindowOptions,
        root: RootFn,
        menus: &[Menu],
        bar: bool,
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
        let id = NEXT_HOST.get();
        NEXT_HOST.set(id + 1);
        let runtime = Runtime::with_waker(move || wake(id));
        let surfaces = Surfaces::new(&gpu).with_waker(move || wake(id));
        let driver = {
            let _entered = runtime.enter();
            let title = options.title;
            let bar = if bar { menus.to_vec() } else { Vec::new() };
            let setup = Setup {
                pixels,
                scale: scale as f32,
                appearance: appearance(&window),
                menus,
                fonts,
                profile: settings,
            };
            Driver::open(gpu, surface, setup, clipboard.clone(), move || {
                window_root(title, bar, false, root)
            })?
        };
        let animate = Closure::new(bind(&slot, Host::draw));
        let timed = bind(&slot, |host, ()| {
            host.timer.set(None);
            host.update(|_, _| {});
        });
        let wake = Closure::new(move || timed(()));
        let measured: Closure<dyn FnMut(Array)> =
            Closure::new(bind(&slot, |host, entries: Array| host.observe(&entries)));
        let observer = ResizeObserver::new(measured.as_ref().unchecked_ref())?;
        let listeners = listeners(&slot, &window, &document, &canvas, &input)?;
        let host = Rc::new(Host {
            mac: is_mac(&window),
            id,
            waking: Cell::new(false),
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
                commands: Vec::new(),
            }),
            runtime,
            clipboard,
            frame: Cell::new(None),
            timer: Cell::new(None),
            wake,
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
        HOSTS.with_borrow_mut(|hosts| hosts.push((id, Rc::downgrade(&host))));
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
        let due = self.due(&mut state);
        drop(state);
        self.schedule_at(due);
    }

    fn schedule_at(&self, due: Option<Instant>) {
        let Some(due) = due else { return };
        let delay = due.saturating_duration_since(Instant::now());
        if delay.is_zero() {
            self.schedule();
            return;
        }
        if let Some(timer) = self.timer.take() {
            self.window.clear_timeout_with_handle(timer);
        }
        let callback = self.wake.as_ref().unchecked_ref();
        let millis = delay.as_millis().try_into().unwrap_or(i32::MAX);
        self.timer.set(
            self.window
                .set_timeout_with_callback_and_timeout_and_arguments_0(callback, millis)
                .ok(),
        );
    }

    fn schedule(&self) {
        if self.frame.get().is_none() && !self.document.hidden() {
            let callback = self.animate.as_ref().unchecked_ref();
            self.frame
                .set(self.window.request_animation_frame(callback).ok());
        }
    }

    fn commands(&self, state: &mut State) {
        let mut commands = take(&mut state.commands);
        state.driver.commands(&mut commands);
        for command in commands.drain(..) {
            match command {
                WindowCommand::SetTitle(title) => self.document.set_title(&title),
                WindowCommand::Focus => {
                    let _ = self.canvas.focus();
                }
                _ => {}
            }
        }
        state.commands = commands;
    }

    fn due(&self, state: &mut State) -> Option<Instant> {
        if self.resized.get() {
            return Some(Instant::now());
        }
        let surfaced = latch(&mut state.surfaces, [&mut state.driver]);
        state.driver.frame_due(surfaced, Instant::now())
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
            let scale = self.window.device_pixel_ratio() as f32;
            if let Some((width, height)) = state.driver.resize(self.pixels.get(), scale) {
                if self.canvas.width() != width {
                    self.canvas.set_width(width);
                }
                if self.canvas.height() != height {
                    self.canvas.set_height(height);
                }
            }
        }
        self.commands(&mut state);
        let State {
            driver, surfaces, ..
        } = &mut *state;
        if driver.draw(surfaces, interval, || {}) {
            self.resized.set(true);
        }
        let due = self.due(&mut state);
        let input = self.sync(&mut state);
        drop(state);
        self.place_input(input, false);
        self.schedule_at(due);
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

    fn event(&self, state: &mut State, source: Source, event: &Event) {
        let name = event.type_();
        let name = name.as_str();
        match (source, name) {
            (Source::Page, "resize" | "scroll") => self.invalidate_rect(),
            (Source::Page, "focus") => state.driver.ui().set_focused(true),
            (Source::Page, "blur") => self.blur(state),
            (Source::Document, _) => self.visibility(state),
            (Source::Scheme, _) => state.driver.ui().set_appearance(appearance(&self.window)),
            (Source::Resolution, _) => {
                self.invalidate_rect();
                self.resized.set(true);
            }
            (Source::Canvas, "pointerleave") => self.leave(state, event),
            (Source::Canvas, "contextmenu") => event.prevent_default(),
            (Source::Canvas, "wheel") => {
                if let Some(event) = event.dyn_ref() {
                    self.wheel(state, event);
                }
            }
            (Source::Canvas, _) if name.starts_with("pointer") || name == "lostpointercapture" => {
                if let Some(event) = event.dyn_ref() {
                    self.pointer(state, name, event);
                }
            }
            (_, "keydown" | "keyup") => {
                if let Some(event) = event.dyn_ref() {
                    self.key(state, name, event);
                }
            }
            (_, "copy" | "cut" | "paste") => {
                if let Some(event) = event.dyn_ref() {
                    self.clipboard_event(state, name, event);
                }
            }
            (Source::Input, "beforeinput") => {
                if let Some(event) = event.dyn_ref() {
                    Self::before_input(state, event);
                }
            }
            (Source::Input, "input") => self.inserted(state, event),
            (Source::Input, _) => {
                if let Some(event) = event.dyn_ref() {
                    self.compose(state, name, event);
                }
            }
            _ => {}
        }
    }

    fn blur(&self, state: &mut State) {
        state.composing = false;
        state.touch = None;
        self.input.set_value("");
        let ui = state.driver.ui();
        ui.set_focused(false);
        ui.pointer_cancel();
        ui.clear_focus();
    }

    fn visibility(&self, state: &mut State) {
        let visible = !self.document.hidden();
        state.driver.ui().set_visible(visible);
        if visible {
            self.resized.set(true);
        }
    }

    fn leave(&self, state: &mut State, event: &Event) {
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

    fn wheel(&self, state: &mut State, event: &WheelEvent) {
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

    fn before_input(state: &mut State, event: &InputEvent) {
        if state.composing || event.is_composing() || !inserts(event) || !event.cancelable() {
            return;
        }
        if let Some(data) = event.data() {
            state.driver.ui().ime(ImeEvent::Commit(data));
            event.prevent_default();
        }
    }

    fn inserted(&self, state: &mut State, event: &Event) {
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

fn listeners(
    slot: &Slot,
    window: &Window,
    document: &Document,
    canvas: &HtmlCanvasElement,
    input: &HtmlTextAreaElement,
) -> Result<Vec<Listener>, AppError> {
    let resolution = format!("(resolution: {}dppx)", window.device_pixel_ratio());
    let mut table: Vec<(EventTarget, Source, &'static [&'static str])> = vec![
        (window.clone().into(), Source::Page, PAGE_EVENTS),
        (document.clone().into(), Source::Document, DOCUMENT_EVENTS),
        (canvas.clone().into(), Source::Canvas, CANVAS_EVENTS),
        (input.clone().into(), Source::Input, INPUT_EVENTS),
    ];
    table.extend(color_scheme(window).map(|query| (query.into(), Source::Scheme, MEDIA_EVENTS)));
    table.extend(
        window
            .match_media(&resolution)?
            .map(|query| (query.into(), Source::Resolution, MEDIA_EVENTS)),
    );
    table
        .into_iter()
        .map(|(target, source, names)| Listener::new(slot, target, source, names))
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
