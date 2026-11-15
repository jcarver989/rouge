#[cfg(target_os = "ios")]
use super::PrimaryTouch;
#[cfg(target_os = "macos")]
use super::menu::NativeMenus;
use super::{
    NativeClipboard, Wheel, WheelGesture, appearance, cursor_icon, logical_point, modifiers,
    named_key, pointer_button, window_command,
};
use crate::app::{App, AppError, RootFn, WindowOptions};
use crate::driver::{Driver, Setup, TextInput, connect, latch};
use crate::platform::{default_menus, window_root};
use crate::profile::{ProfileSettings, subscribe};
use pollster::block_on;
use rouge_core::{
    Decorations, FileDropPhase, ImeEvent, Key, KeyEvent, Menu, Modifiers, WindowCommand,
};
use rouge_reactive::Runtime;
#[cfg(feature = "tokio-rt")]
use rouge_reactive::set_tokio;
#[cfg(target_os = "ios")]
use rouge_render::Edges;
use rouge_render::{Gpu, Point, Surfaces};
use std::borrow::Cow;
use std::env::var;
use std::mem::take;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::level_filters::LevelFilter;
use tracing::trace_span;
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::Layer;
use wgpu::{Instance, InstanceDescriptor};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Ime, KeyEvent as WinitKeyEvent, MouseScrollDelta, TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, SmolStr};
#[cfg(not(target_os = "ios"))]
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::{Window, WindowId};

pub(crate) fn run(app: App) -> Result<(), AppError> {
    let bar = !app.menus.is_empty();
    let menus = default_menus(app.menus);
    let event_loop = EventLoop::new()?;
    let waker = event_loop.create_proxy();
    #[cfg(target_os = "macos")]
    let native_menus = NativeMenus::new(menus.clone(), waker.clone())?;
    let proxy = waker.clone();
    let runtime = Runtime::with_waker(move || {
        let _ = proxy.send_event(());
    });
    let _entered = runtime.enter();
    #[cfg(feature = "tokio-rt")]
    if let Some(handle) = app.tokio {
        set_tokio(handle);
    }
    event_loop.set_control_flow(ControlFlow::Wait);
    let profile = ProfileSettings::read(|name| var(name.to_ascii_uppercase()).ok());
    let mut runner = Runner {
        #[cfg(target_os = "macos")]
        native_menus,
        _trace: trace(&profile),
        waker,
        gpu: None,
        surfaces: None,
        pending: app.windows,
        menus,
        bar,
        fonts: app.fonts,
        profile,
        windows: Vec::new(),
        commands: Vec::new(),
        error: None,
    };
    event_loop.run_app(&mut runner)?;
    runner.error.map_or(Ok(()), Err)
}

struct Runner {
    #[cfg(target_os = "macos")]
    native_menus: NativeMenus,
    _trace: Option<FlushGuard>,
    waker: EventLoopProxy<()>,
    gpu: Option<(Instance, Gpu)>,
    surfaces: Option<Surfaces>,
    pending: Vec<(WindowOptions, RootFn)>,
    menus: Vec<Menu>,
    bar: bool,
    fonts: Vec<Cow<'static, [u8]>>,
    profile: ProfileSettings,
    windows: Vec<WindowState>,
    commands: Vec<WindowCommand>,
    error: Option<AppError>,
}

impl Runner {
    fn open(
        &mut self,
        event_loop: &ActiveEventLoop,
        options: WindowOptions,
        root: RootFn,
    ) -> Result<(), AppError> {
        let mut attributes = Window::default_attributes()
            .with_title(options.title.clone())
            .with_maximized(options.maximized)
            .with_decorations(options.decorations == Decorations::Server)
            .with_resizable(options.resizable);
        if !cfg!(target_os = "ios") {
            attributes = attributes
                .with_inner_size(LogicalSize::new(options.size.width, options.size.height));
            if let Some(size) = options.min_size {
                attributes =
                    attributes.with_min_inner_size(LogicalSize::new(size.width, size.height));
            }
        }
        let window = Arc::new(event_loop.create_window(attributes)?);
        let instance = || {
            let display = Box::new(event_loop.owned_display_handle());
            Instance::new(InstanceDescriptor::new_with_display_handle_from_env(
                display,
            ))
        };
        let (gpu, surface) = block_on(connect(&mut self.gpu, instance, window.clone()))?;
        let waker = self.waker.clone();
        self.surfaces.get_or_insert_with(|| {
            Surfaces::new(&gpu).with_waker(move || {
                let _ = waker.send_event(());
            })
        });
        let physical = surface_size(&window);
        let chrome = !cfg!(target_os = "ios")
            && (options.decorations == Decorations::Client || !window.is_decorated());
        let bar = if self.bar {
            self.menus.clone()
        } else {
            Vec::new()
        };
        let title = options.title;
        let setup = Setup {
            pixels: (physical.width, physical.height),
            scale: window.scale_factor() as f32,
            appearance: appearance(window.theme()),
            menus: &self.menus,
            fonts: &self.fonts,
            profile: &self.profile,
        };
        let driver = Driver::open(gpu, surface, setup, NativeClipboard::new(), move || {
            window_root(title, bar, chrome, root)
        })?;
        let interval = refresh_interval(&window);
        self.windows.push(WindowState {
            window,
            driver,
            interval,
            occluded: false,
            wheel: Wheel::default(),
            #[cfg(target_os = "ios")]
            touch: PrimaryTouch::default(),
            modifiers: Modifiers::NONE,
        });
        #[cfg(target_os = "ios")]
        if let Some(state) = self.windows.last_mut() {
            state.sync_safe_area();
        }
        Ok(())
    }

    fn process_window_commands(&mut self, event_loop: &ActiveEventLoop, only: Option<WindowId>) {
        let mut quit = false;
        let commands = &mut self.commands;
        self.windows.retain_mut(|state| {
            if only.is_some_and(|id| id != state.window.id()) {
                return true;
            }
            let mut open = true;
            state.driver.commands(commands);
            for command in commands.drain(..) {
                match command {
                    WindowCommand::Close => open = false,
                    WindowCommand::Quit => quit = true,
                    command => window_command(&state.window, command),
                }
            }
            open
        });
        if quit || self.windows.is_empty() {
            self.windows.clear();
            event_loop.exit();
        }
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        for (options, root) in take(&mut self.pending) {
            if let Err(error) = self.open(event_loop, options, root) {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.windows.retain(|state| !state.driver.finished());
        let surfaced = self.surfaces.as_mut().is_some_and(|surfaces| {
            latch(
                surfaces,
                self.windows.iter_mut().map(|state| &mut state.driver),
            )
        });
        let now = Instant::now();
        let mut deadline = self.surfaces.as_ref().and_then(Surfaces::next_deadline);
        for state in self.windows.iter_mut().filter(|state| !state.occluded) {
            match state.driver.frame_due(surfaced, now) {
                Some(due) if due <= now => state.window.request_redraw(),
                Some(due) => deadline = Some(deadline.map_or(due, |at| at.min(due))),
                None => {}
            }
        }
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
        self.process_window_commands(event_loop, None);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, (): ()) {
        #[cfg(target_os = "macos")]
        {
            let ui = self
                .windows
                .iter_mut()
                .find(|state| state.window.has_focus())
                .map(|state| state.driver.ui());
            if self.native_menus.dispatch(ui) {
                self.windows.clear();
            }
        }
        self.process_window_commands(event_loop, None);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(state) = self
            .windows
            .iter_mut()
            .find(|state| state.window.id() == id)
        else {
            return;
        };
        let _event = trace_span!("event", ?event).entered();
        match event {
            WindowEvent::CloseRequested => {
                self.windows.retain(|state| state.window.id() != id);
            }
            WindowEvent::RedrawRequested => {
                if !state.occluded
                    && let Some(surfaces) = &mut self.surfaces
                {
                    state.redraw(surfaces);
                }
                if state.driver.finished() {
                    self.windows.clear();
                }
            }
            event => state.event(event),
        }
        if let Some(state) = self
            .windows
            .iter_mut()
            .find(|state| state.window.id() == id)
        {
            state.sync_platform();
        }
        self.process_window_commands(event_loop, Some(id));
    }
}

struct WindowState {
    window: Arc<Window>,
    driver: Driver,
    interval: Duration,
    occluded: bool,
    wheel: Wheel,
    #[cfg(target_os = "ios")]
    touch: PrimaryTouch,
    modifiers: Modifiers,
}

impl WindowState {
    fn event(&mut self, event: WindowEvent) {
        let ui = self.driver.ui();
        match event {
            WindowEvent::ThemeChanged(theme) => ui.set_appearance(appearance(Some(theme))),
            WindowEvent::Focused(focused) => ui.set_focused(focused),
            WindowEvent::HoveredFile(path) => {
                ui.file_drop(FileDropPhase::Hover, vec![path]);
            }
            WindowEvent::HoveredFileCancelled => {
                ui.file_drop(FileDropPhase::Leave, Vec::new());
            }
            WindowEvent::DroppedFile(path) => {
                ui.file_drop(FileDropPhase::Drop, vec![path]);
            }
            WindowEvent::Resized(size) => self.resize(size, self.window.scale_factor()),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.interval = refresh_interval(&self.window);
                self.resize(surface_size(&self.window), scale_factor);
            }
            WindowEvent::Moved(_) => self.interval = refresh_interval(&self.window),
            WindowEvent::CursorMoved { position, .. } => {
                let position = logical_point(position, ui.scale());
                ui.pointer_moved(position);
            }
            WindowEvent::CursorLeft { .. } => ui.pointer_left(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => ui.pointer_down(pointer_button(button)),
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button,
                ..
            } => ui.pointer_up(pointer_button(button)),
            WindowEvent::MouseWheel { delta, phase, .. } => self.wheel(delta, phase),
            #[cfg(target_os = "ios")]
            WindowEvent::Touch(touch) => {
                if let Some(phase) = self.touch.input(touch.id, touch.phase) {
                    let position = logical_point(touch.location, ui.scale());
                    ui.touch(phase, position);
                }
            }
            WindowEvent::ModifiersChanged(changed) => {
                self.modifiers = modifiers(changed.state());
                ui.set_modifiers(self.modifiers);
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => self.key(event),
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                ui.ime(ImeEvent::Preedit { text, cursor });
            }
            WindowEvent::Ime(Ime::Commit(text)) => ui.ime(ImeEvent::Commit(text)),
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                ui.set_visible(!occluded);
                if !occluded {
                    self.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn key(&mut self, event: WinitKeyEvent) {
        let key = match unmodified_key(&event) {
            WinitKey::Named(named) => named_key(named),
            WinitKey::Character(text) => Key::character(&text),
            _ => Key::Other,
        };
        let mut key_event = KeyEvent::new(key).with_modifiers(self.modifiers);
        key_event.text = event.text.as_ref().map(SmolStr::to_string);
        key_event.repeat = event.repeat;
        let ui = self.driver.ui();
        match event.state {
            ElementState::Pressed => ui.key_down(key_event),
            ElementState::Released => ui.key_up(key_event),
        };
    }

    fn sync_platform(&mut self) {
        let changes = self.driver.changes();
        if let Some(cursor) = changes.cursor {
            self.window.set_cursor(cursor_icon(cursor));
        }
        if let Some(text_input) = changes.text_input {
            self.window
                .set_ime_allowed(matches!(text_input, TextInput::On { .. }));
            if let TextInput::On { area: Some(area) } = text_input {
                self.window.set_ime_cursor_area(
                    PhysicalPosition::new(area.origin.x, area.origin.y),
                    PhysicalSize::new(area.size.width, area.size.height),
                );
            }
        }
    }

    fn resize(&mut self, physical: PhysicalSize<u32>, scale: f64) {
        self.driver
            .resize((physical.width, physical.height), scale as f32);
        #[cfg(target_os = "ios")]
        self.sync_safe_area();
        self.window.request_redraw();
    }

    #[cfg(target_os = "ios")]
    fn sync_safe_area(&mut self) {
        let insets = safe_area(&self.window);
        self.driver.ui().set_safe_area(insets);
    }

    fn wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase) {
        let ui = self.driver.ui();
        match self.wheel.input(delta, phase, ui.scale(), Instant::now()) {
            WheelGesture::Scroll(by) => {
                ui.scroll(by);
            }
            WheelGesture::Drag(by) => ui.scroll_drag(by),
            WheelGesture::Release(by) => {
                if by != Point::ZERO {
                    ui.scroll_drag(by);
                }
                ui.scroll_release();
            }
            WheelGesture::Coast => {}
        }
    }

    fn redraw(&mut self, surfaces: &mut Surfaces) {
        let window = &self.window;
        if self
            .driver
            .draw(surfaces, self.interval, || window.pre_present_notify())
        {
            window.request_redraw();
        }
    }
}

#[cfg(not(target_os = "ios"))]
fn surface_size(window: &Window) -> PhysicalSize<u32> {
    window.inner_size()
}

#[cfg(target_os = "ios")]
fn surface_size(window: &Window) -> PhysicalSize<u32> {
    window.outer_size()
}

#[cfg(target_os = "ios")]
fn safe_area(window: &Window) -> Edges {
    let (Ok(inner), Ok(outer)) = (window.inner_position(), window.outer_position()) else {
        return Edges::ZERO;
    };
    let (inner_size, outer_size) = (window.inner_size(), window.outer_size());
    let scale = window.scale_factor() as f32;
    let left = (inner.x - outer.x) as f32;
    let top = (inner.y - outer.y) as f32;
    let right = outer_size.width as f32 - inner_size.width as f32 - left;
    let bottom = outer_size.height as f32 - inner_size.height as f32 - top;
    Edges {
        top: (top / scale).max(0.0),
        right: (right / scale).max(0.0),
        bottom: (bottom / scale).max(0.0),
        left: (left / scale).max(0.0),
    }
}

#[cfg(not(target_os = "ios"))]
fn unmodified_key(event: &WinitKeyEvent) -> WinitKey {
    event.key_without_modifiers()
}

#[cfg(target_os = "ios")]
fn unmodified_key(event: &WinitKeyEvent) -> WinitKey {
    event.logical_key.clone()
}

fn refresh_interval(window: &Window) -> Duration {
    let millihertz = window
        .current_monitor()
        .and_then(|monitor| monitor.refresh_rate_millihertz())
        .filter(|rate| *rate > 0)
        .unwrap_or(60_000);
    Duration::from_secs_f64(1000.0 / f64::from(millihertz))
}

fn trace(settings: &ProfileSettings) -> Option<FlushGuard> {
    let path = settings.trace.as_deref().filter(|path| !path.is_empty())?;
    let (layer, guard) = ChromeLayerBuilder::new()
        .file(path)
        .include_args(true)
        .build();
    let layer = layer.with_filter(
        Targets::new()
            .with_target("winit", LevelFilter::OFF)
            .with_default(LevelFilter::TRACE),
    );
    subscribe(layer).then(|| {
        eprintln!("rouge profile: tracing to {path}");
        guard
    })
}
