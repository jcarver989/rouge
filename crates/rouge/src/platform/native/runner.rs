use super::{
    Input, UserEvent, Wheel, WheelGesture, WinitPlatform, appearance, cursor_icon, logical_point,
    modifiers, named_key, pointer_button, touch_phase, window_command,
};
use crate::app::{App, AppError, RootFn, WindowOptions};
use crate::changes::Tracked;
use crate::compositing::Compositing;
use crate::driver::{Driver, Setup, connect};
use crate::platform::{Current, Platform, PrimaryTouch, default_menus, window_root};
use crate::profile::{ProfileSettings, trace};
use accesskit_winit::{Adapter, WindowEvent as AccessEvent};
use pollster::block_on;
use rouge_core::access::TreeUpdate;
use rouge_core::{
    CompositorEvent, Decorations, FileDropPhase, ImeEvent, InputKind, Key, KeyEvent, Menu,
    Modifiers, WindowCommand, WindowId as LinkWindow,
};
use rouge_reactive::Runtime;
#[cfg(feature = "tokio-rt")]
use rouge_reactive::set_tokio;
use rouge_render::{Gpu, Point, Target};
use std::borrow::Cow;
use std::cell::RefCell;
use std::env::var;
use std::mem::take;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::trace_span;
use tracing_chrome::FlushGuard;
use wgpu::{Instance, InstanceDescriptor};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{
    ElementState, Ime, KeyEvent as WinitKeyEvent, MouseScrollDelta, TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, SmolStr};
use winit::window::{Icon, Window, WindowId};

pub(crate) fn run(app: App) -> Result<(), AppError> {
    let bar = !app.menus.is_empty();
    let event_loop = EventLoop::with_user_event().build()?;
    let menus = default_menus(app.menus);
    let waker = event_loop.create_proxy();
    let platform = Current::new(&menus, &waker)?;
    let proxy = waker.clone();
    let runtime = Runtime::with_waker(move || {
        let _ = proxy.send_event(None);
    });
    let _entered = runtime.enter();
    #[cfg(feature = "tokio-rt")]
    if let Some(handle) = app.tokio {
        set_tokio(handle);
    }
    event_loop.set_control_flow(ControlFlow::Wait);
    let profile = ProfileSettings::read(|name| var(name.to_ascii_uppercase()).ok());
    let mut runner = Runner {
        platform,
        _trace: trace(&profile),
        waker,
        gpu: None,
        compositor: None,
        next_window: 0,
        closing: Vec::new(),
        pending: app.windows,
        pending_icon: app.icon,
        window_icon: None,
        menus,
        bar,
        fonts: app.fonts,
        profile,
        windows: Vec::new(),
        commands: Vec::new(),
        error: None,
    };
    event_loop.run_app(&mut runner)?;
    let closed = runner.compositor.take().map_or(Ok(()), Compositing::close);
    runner.windows.clear();
    runner.closing.clear();
    runner.error.map_or(closed, Err)
}

struct Runner {
    platform: Current,
    _trace: Option<FlushGuard>,
    waker: EventLoopProxy<UserEvent>,
    gpu: Option<(Instance, Gpu)>,
    compositor: Option<Compositing>,
    next_window: u64,
    closing: Vec<(LinkWindow, Arc<Window>)>,
    pending: Vec<(WindowOptions, RootFn)>,
    pending_icon: Option<Cow<'static, [u8]>>,
    window_icon: Option<Icon>,
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
            .with_visible(false)
            .with_title(options.title.clone())
            .with_maximized(options.maximized)
            .with_decorations(options.decorations == Decorations::Server)
            .with_resizable(options.resizable)
            .with_window_icon(self.window_icon.clone());
        if !Current::MOBILE {
            attributes = attributes
                .with_inner_size(LogicalSize::new(options.size.width, options.size.height));
            if let Some(size) = options.min_size {
                attributes =
                    attributes.with_min_inner_size(LogicalSize::new(size.width, size.height));
            }
        }
        let window = Arc::new(event_loop.create_window(attributes)?);
        let access = Adapter::with_event_loop_proxy(event_loop, &window, self.waker.clone());
        window.set_visible(true);
        let instance = async {
            let display = Box::new(event_loop.owned_display_handle());
            Instance::new(InstanceDescriptor::new_with_display_handle_from_env(
                display,
            ))
        };
        let (gpu, surface) = block_on(connect(&mut self.gpu, instance, window.clone()))?;
        let waker = self.waker.clone();
        let wake = move || {
            let _ = waker.send_event(None);
        };
        let compositor = match &self.compositor {
            Some(compositor) => compositor,
            None => (self.compositor).insert(Compositing::open(&gpu, wake)?),
        };
        let physical = Current::surface_size(&window);
        let chrome = !Current::MOBILE
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
            interval: refresh_interval(&window),
            epoch: compositor.epoch,
            appearance: Current::appearance(&window),
            input: if Current::MOBILE {
                InputKind::Touch
            } else {
                InputKind::Pointer
            },
            menus: &self.menus,
            fonts: &self.fonts,
            profile: &self.profile,
        };
        let id = LinkWindow(self.next_window);
        self.next_window += 1;
        let (driver, target) = Driver::open(
            id,
            &gpu,
            surface,
            setup,
            compositor.messages.clone(),
            Current::clipboard(),
            move || window_root(title, bar, chrome, root),
        )?;
        driver.attach(present_target(compositor, target, &window));
        let driver = Rc::new(RefCell::new(driver));
        let input = <Current as WinitPlatform>::Input::attach(&window, &driver, &self.waker);
        self.windows.push(WindowState {
            window,
            driver,
            tracked: Tracked::default(),
            input,
            access,
            occluded: false,
            wheel: Wheel::default(),
            touch: PrimaryTouch::default(),
            modifiers: Modifiers::NONE,
        });
        if let Some(state) = self.windows.last_mut() {
            state.sync_environment();
        }
        Ok(())
    }

    fn pump(&mut self) {
        let events = self.compositor.iter().flat_map(|c| c.events.try_iter());
        events.for_each(|event| deliver(&mut self.windows, &mut self.closing, event));
    }

    fn settle(&mut self, window: LinkWindow, timeout: Duration) {
        let Some(compositor) = &mut self.compositor else {
            return;
        };
        compositor.turn();
        let deadline = Instant::now() + timeout;
        while compositor.threaded()
            && let Some(left) = deadline.checked_duration_since(Instant::now())
            && let Ok(event) = compositor.events.recv_timeout(left)
        {
            let presented = event == (window, CompositorEvent::Presented);
            deliver(&mut self.windows, &mut self.closing, event);
            if presented {
                break;
            }
        }
        self.pump();
    }

    fn process_window_commands(&mut self, event_loop: &ActiveEventLoop, only: Option<WindowId>) {
        let mut quit = false;
        let commands = &mut self.commands;
        let closing = &mut self.closing;
        self.windows.retain_mut(|state| {
            if only.is_some_and(|id| id != state.window.id()) {
                return true;
            }
            let mut open = true;
            state.driver.borrow_mut().commands(commands);
            for command in commands.drain(..) {
                match command {
                    WindowCommand::Close => open = false,
                    WindowCommand::Quit => quit = true,
                    command => window_command(&state.window, command),
                }
            }
            if !open {
                state.close(closing);
            }
            open
        });
        if quit || (self.windows.is_empty() && self.pending.is_empty()) {
            self.windows.clear();
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<UserEvent> for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(png) = self.pending_icon.take() {
            self.window_icon = Current::app_icon(&png);
        }
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
            .retain(|state| !state.driver.borrow().finished());
        for state in &mut self.windows {
            state.sync_environment();
        }
        let mut deadline = self.compositor.as_mut().and_then(Compositing::turn);
        self.pump();
        let now = Instant::now();
        for state in self.windows.iter_mut().filter(|state| !state.occluded) {
            let due = state.driver.borrow_mut().frame_due(now);
            match due {
                Some(due) if due <= now => state.window.request_redraw(),
                Some(due) => deadline = Some(deadline.map_or(due, |at| at.min(due))),
                None => {}
            }
        }
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
        self.process_window_commands(event_loop, None);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        if let Some(event) = event
            && let Some(state) = self
                .windows
                .iter_mut()
                .find(|s| s.window.id() == event.window_id)
        {
            state.assist(event.window_event);
        }
        let mut driver = self
            .windows
            .iter()
            .find(|state| state.window.has_focus())
            .map(|state| state.driver.borrow_mut());
        let quit = self
            .platform
            .menu_events(driver.as_mut().map(|driver| driver.ui()));
        drop(driver);
        if quit {
            self.windows.clear();
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
        state.access.process_event(&state.window, &event);
        match event {
            WindowEvent::CloseRequested => {
                state.close(&mut self.closing);
                self.windows.retain(|state| state.window.id() != id);
            }
            WindowEvent::RedrawRequested => {
                if !state.occluded {
                    state.step();
                }
                if state.driver.borrow().finished() {
                    self.windows.clear();
                }
            }
            event => {
                let resized = matches!(
                    event,
                    WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
                );
                state.event(event);
                if resized && !state.occluded {
                    let (window, timeout) = {
                        let driver = state.driver.borrow();
                        (driver.window(), driver.interval() * 2)
                    };
                    self.settle(window, timeout);
                }
            }
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
    driver: Rc<RefCell<Driver>>,
    tracked: Tracked,
    input: <Current as WinitPlatform>::Input,
    access: Adapter,
    occluded: bool,
    wheel: Wheel,
    touch: PrimaryTouch,
    modifiers: Modifiers,
}

impl WindowState {
    fn close(&self, closing: &mut Vec<(LinkWindow, Arc<Window>)>) {
        let driver = self.driver.borrow();
        driver.close();
        closing.push((driver.window(), self.window.clone()));
    }

    fn event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::Resized(size) => self.resize(size, self.window.scale_factor()),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.refresh_interval();
                self.resize(Current::surface_size(&self.window), scale_factor);
            }
            WindowEvent::Moved(_) => self.refresh_interval(),
            WindowEvent::MouseWheel { delta, phase, .. } => self.wheel(delta, phase),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => self.key(event),
            event => self.input(event),
        }
    }

    fn input(&mut self, event: WindowEvent) {
        let mut driver = self.driver.borrow_mut();
        let ui = driver.ui();
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
            WindowEvent::Touch(touch) if Current::MOBILE => {
                if let Some(phase) = self.touch.input(touch.id, touch_phase(touch.phase)) {
                    let position = logical_point(touch.location, ui.scale());
                    ui.touch(phase, position);
                }
            }
            WindowEvent::ModifiersChanged(changed) => {
                self.modifiers = modifiers(changed.state());
                ui.set_modifiers(self.modifiers);
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                ui.ime(ImeEvent::Preedit { text, cursor });
            }
            WindowEvent::Ime(Ime::Commit(text)) => ui.ime(ImeEvent::Commit(text)),
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                ui.set_visible(!occluded);
                driver.configure(|config| config.visible = !occluded);
                if !occluded {
                    self.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn key(&mut self, event: WinitKeyEvent) {
        let key = match Current::key(&event) {
            WinitKey::Named(named) => named_key(named),
            WinitKey::Character(text) => Key::character(&text),
            _ => Key::Other,
        };
        let mut key_event = KeyEvent::new(key).with_modifiers(self.modifiers);
        key_event.text = event.text.as_ref().map(SmolStr::to_string);
        key_event.repeat = event.repeat;
        let mut driver = self.driver.borrow_mut();
        let ui = driver.ui();
        match event.state {
            ElementState::Pressed => ui.key_down(key_event),
            ElementState::Released => ui.key_up(key_event),
        };
    }

    fn sync_platform(&mut self) {
        let changes = self.tracked.changes(self.driver.borrow_mut().ui());
        if let Some(cursor) = changes.cursor {
            self.window.set_cursor(cursor_icon(cursor));
        }
        self.input.sync(&self.window, changes.text_input);
    }

    fn refresh_interval(&mut self) {
        let interval = refresh_interval(&self.window);
        self.driver
            .borrow_mut()
            .configure(|config| config.interval = interval);
    }

    fn resize(&mut self, physical: PhysicalSize<u32>, scale: f64) {
        let resized = self
            .driver
            .borrow_mut()
            .resize((physical.width, physical.height), scale as f32);
        if resized.is_some() && !self.occluded {
            self.step();
        }
        self.window.request_redraw();
    }

    fn step(&mut self) {
        let update = self.driver.borrow_mut().step();
        self.publish(update);
    }

    fn publish(&mut self, update: Option<TreeUpdate>) {
        if let Some(update) = update {
            self.access.update_if_active(|| update);
        }
    }

    fn assist(&mut self, request: AccessEvent) {
        let mut driver = self.driver.borrow_mut();
        let ui = driver.ui();
        match request {
            AccessEvent::InitialTreeRequested => {
                self.access.update_if_active(|| ui.accessibility_tree());
            }
            AccessEvent::ActionRequested(request) => {
                ui.accessibility_action(request);
            }
            AccessEvent::AccessibilityDeactivated => ui.stop_accessibility(),
        }
        let update = self.occluded.then(|| driver.refresh()).flatten();
        drop(driver);
        self.publish(update);
    }

    fn sync_environment(&mut self) {
        let keyboard = self.input.keyboard_inset(&self.window);
        let mut driver = self.driver.borrow_mut();
        let ui = driver.ui();
        Current::sync(&self.window, ui);
        if Current::MOBILE {
            ui.set_safe_area(Current::safe_area(&self.window));
            ui.set_keyboard_inset(keyboard);
        }
    }

    fn wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase) {
        let mut driver = self.driver.borrow_mut();
        let ui = driver.ui();
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
}

fn present_target(compositor: &Compositing, target: Target, window: &Arc<Window>) -> Target {
    if compositor.threaded() {
        return target;
    }
    let notify = window.clone();
    target.before_present(move || notify.pre_present_notify())
}

fn deliver(
    windows: &mut [WindowState],
    closing: &mut Vec<(LinkWindow, Arc<Window>)>,
    (id, event): (LinkWindow, CompositorEvent),
) {
    if event == CompositorEvent::Closed {
        return closing.retain(|(window, _)| *window != id);
    }
    if let Some(state) = windows
        .iter_mut()
        .find(|state| state.driver.borrow().window() == id)
    {
        let mut driver = state.driver.borrow_mut();
        driver.handle(event);
        if driver.frame_due(Instant::now()).is_some() {
            state.window.request_redraw();
        }
    }
}

fn refresh_interval(window: &Window) -> Duration {
    let millihertz = Current::refresh_millihertz(window)
        .filter(|rate| *rate > 0)
        .unwrap_or(60_000);
    Duration::from_secs_f64(1000.0 / f64::from(millihertz))
}
