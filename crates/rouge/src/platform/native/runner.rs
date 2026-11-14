use super::clipboard::NativeClipboard;
#[cfg(target_os = "macos")]
use super::menu::NativeMenus;
use super::{
    Wheel, WheelGesture, appearance, cursor_icon, logical_point, logical_size, modifiers,
    named_key, pointer_button, warn, window_command,
};
use crate::app::{App, AppError, RootFn, WindowOptions};
use crate::driver::{Drawn, Driver, TextInput, connect, latch};
use crate::platform::window_root;
use crate::profile::{ProfileSettings, subscribe};
use pollster::block_on;
use rouge_core::{
    Decorations, FileDropPhase, ImeEvent, Key, KeyEvent, Menu, Modifiers, Ui, WindowCommand,
};
use rouge_reactive::Runtime;
#[cfg(feature = "tokio-rt")]
use rouge_reactive::set_tokio;
use rouge_render::{Gpu, Point, Surfaces};
use std::borrow::Cow;
use std::collections::HashMap;
use std::env::var;
use std::mem::take;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::level_filters::LevelFilter;
use tracing::trace_span;
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::Layer;
use wgpu::{Instance, InstanceDescriptor, PresentMode};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Ime, KeyEvent as WinitKeyEvent, MouseScrollDelta, TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, SmolStr};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::{Window, WindowId};

pub(crate) fn run(mut app: App) -> Result<(), AppError> {
    let has = |menus: &[Menu], title: &str| menus.iter().any(|menu| menu.title == title);
    if !has(&app.menus, "Edit") {
        app.menus.insert(app.menus.len().min(1), Menu::edit());
    }
    if !has(&app.menus, "Window") {
        app.menus.push(Menu::window());
    }
    let event_loop = EventLoop::new()?;
    let waker = event_loop.create_proxy();
    #[cfg(target_os = "macos")]
    let native_menus = NativeMenus::new(app.menus.clone(), waker.clone())?;
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
        menus: app.menus,
        fonts: app.fonts,
        profile,
        present_mode: present_mode(),
        windows: HashMap::new(),
        error: None,
        exit_after_frames: var("ROUGE_EXIT_AFTER_FRAMES")
            .ok()
            .and_then(|v| v.parse().ok()),
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
    fonts: Vec<Cow<'static, [u8]>>,
    profile: ProfileSettings,
    present_mode: Option<PresentMode>,
    windows: HashMap<WindowId, WindowState>,
    error: Option<AppError>,
    exit_after_frames: Option<u64>,
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
            .with_resizable(options.resizable)
            .with_inner_size(LogicalSize::new(options.size.width, options.size.height));
        if let Some(size) = options.min_size {
            attributes = attributes.with_min_inner_size(LogicalSize::new(size.width, size.height));
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
        let physical = window.inner_size();
        let scale = window.scale_factor() as f32;
        let chrome = options.decorations == Decorations::Client || !window.is_decorated();
        let menus = self.menus.clone();
        let title = options.title;
        let mut ui = Ui::with_fonts(
            logical_size(physical, scale),
            self.fonts.iter().cloned(),
            move || window_root(title, menus, chrome, root),
        );
        ui.set_appearance(appearance(window.theme()));
        ui.set_device_scale(scale);
        ui.set_clipboard(NativeClipboard::new());
        ui.bind_menus(&self.menus);
        let driver = Driver::new(
            gpu,
            surface,
            (physical.width, physical.height),
            ui,
            options.background,
            self.present_mode,
            &self.profile,
        )?;
        let state = WindowState {
            window,
            driver,
            occluded: false,
            wheel: Wheel::default(),
            modifiers: Modifiers::NONE,
        };
        self.windows.insert(state.window.id(), state);
        Ok(())
    }

    fn process_window_commands(&mut self, event_loop: &ActiveEventLoop) {
        let mut quit = false;
        self.windows.retain(|_, state| {
            let mut open = true;
            for command in state.driver.commands() {
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
        self.windows
            .retain(|_, state| !state.driver.profile_finished());
        let surfaced = self.surfaces.as_mut().is_some_and(|surfaces| {
            latch(
                surfaces,
                self.windows.values_mut().map(|state| &mut state.driver),
            )
        });
        for state in self.windows.values_mut() {
            if !state.occluded && state.driver.wants_frame(surfaced) {
                state.window.request_redraw();
            }
        }
        let deadline = self.surfaces.as_ref().and_then(Surfaces::next_deadline);
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
        self.process_window_commands(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, (): ()) {
        #[cfg(target_os = "macos")]
        {
            let ui = self
                .windows
                .values_mut()
                .find(|state| state.window.has_focus())
                .map(|state| state.driver.ui());
            if self.native_menus.dispatch(ui) {
                self.windows.clear();
            }
        }
        self.process_window_commands(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(state) = self.windows.get_mut(&id) else {
            return;
        };
        let _event = trace_span!("event", ?event).entered();
        match event {
            WindowEvent::CloseRequested => {
                self.windows.remove(&id);
            }
            WindowEvent::RedrawRequested => {
                if !state.occluded
                    && let Some(surfaces) = &mut self.surfaces
                {
                    state.redraw(surfaces);
                }
                if self
                    .exit_after_frames
                    .is_some_and(|n| state.driver.frames() >= n)
                {
                    self.windows.clear();
                }
            }
            event => state.event(event),
        }
        if let Some(state) = self.windows.get_mut(&id) {
            state.sync_platform();
        }
        self.process_window_commands(event_loop);
    }
}

struct WindowState {
    window: Arc<Window>,
    driver: Driver,
    occluded: bool,
    wheel: Wheel,
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
                self.resize(self.window.inner_size(), scale_factor);
            }
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
        let key = match event.key_without_modifiers() {
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
        self.window.request_redraw();
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
        let millihertz = self
            .window
            .current_monitor()
            .and_then(|monitor| monitor.refresh_rate_millihertz())
            .filter(|rate| *rate > 0)
            .unwrap_or(60_000);
        let interval = Duration::from_secs_f64(1000.0 / f64::from(millihertz));
        let window = &self.window;
        if self
            .driver
            .draw(surfaces, interval, || window.pre_present_notify())
            == Drawn::Retry
        {
            window.request_redraw();
        }
    }
}

impl Drop for WindowState {
    fn drop(&mut self) {
        if let Some(summary) = self.driver.profile_summary() {
            eprintln!("{summary}");
        }
    }
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

fn present_mode() -> Option<PresentMode> {
    let mode = var("ROUGE_PRESENT_MODE").ok()?;
    match mode.to_ascii_lowercase().as_str() {
        "fifo" => Some(PresentMode::Fifo),
        "fifo-relaxed" => Some(PresentMode::FifoRelaxed),
        "mailbox" => Some(PresentMode::Mailbox),
        "immediate" => Some(PresentMode::Immediate),
        "auto-vsync" => Some(PresentMode::AutoVsync),
        "auto-no-vsync" => Some(PresentMode::AutoNoVsync),
        _ => {
            warn(&format!("unknown ROUGE_PRESENT_MODE `{mode}`"));
            None
        }
    }
}
