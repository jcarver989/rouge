use crate::platform::{
    appearance as platform_appearance, cursor_icon, logical_point, logical_size, modifiers,
    named_key, pointer_button, scroll_delta, window_commands,
};
use arboard::Clipboard as SystemClipboard;
use pollster::block_on;
use rouge_core::{
    AnyView, Appearance, Clipboard, Cursor, Decorations, ImeEvent, Key, KeyEvent, Menu, Modifiers,
    Ui, View, appearance, window_handle,
};
use rouge_reactive::{ReadSignal, Runtime};
use rouge_render::{Color, Gpu, GpuError, Point, Renderer, Size, Viewport};
use rouge_widgets::window_chrome;
use std::collections::HashMap;
use std::env::var;
use std::mem::take;
use std::sync::Arc;
use thiserror::Error;
use wgpu::{
    CreateSurfaceError, CurrentSurfaceTexture, Instance, InstanceDescriptor, PresentMode, Surface,
    SurfaceConfiguration, TextureViewDescriptor,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
use winit::error::{EventLoopError, OsError};
use winit::event::{
    ElementState, Ime, KeyEvent as WinitKeyEvent, MouseScrollDelta, TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, SmolStr};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::{Window, WindowId};

#[derive(Clone, Debug)]
pub struct WindowOptions {
    title: String,
    size: Size,
    background: Color,
    resizable: bool,
    min_size: Option<Size>,
    maximized: bool,
    decorations: Decorations,
}

impl Default for WindowOptions {
    fn default() -> Self {
        WindowOptions {
            title: "Rouge".into(),
            size: Size::new(800.0, 600.0),
            background: Color::WHITE,
            resizable: true,
            min_size: None,
            maximized: false,
            decorations: Decorations::Server,
        }
    }
}

impl WindowOptions {
    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = Some(Size::new(width, height));
        self
    }
    pub fn maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }
    pub fn decorations(mut self, decorations: Decorations) -> Self {
        self.decorations = decorations;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = Size::new(width, height);
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("event loop error: {0}")]
    EventLoop(#[from] EventLoopError),
    #[error("failed to create window: {0}")]
    Window(#[from] OsError),
    #[error("failed to create rendering surface: {0}")]
    Surface(#[from] CreateSurfaceError),
    #[error("the GPU adapter cannot present to the window surface")]
    UnsupportedSurface,
    #[cfg(target_os = "macos")]
    #[error("failed to create application menu: {0}")]
    Menu(#[from] muda::Error),
    #[error(transparent)]
    Gpu(#[from] GpuError),
}

type RootFn = Box<dyn FnOnce() -> AnyView>;

#[derive(Default)]
pub struct App {
    windows: Vec<(WindowOptions, RootFn)>,
    menus: Vec<Menu>,
    fonts: Vec<Vec<u8>>,
}

impl App {
    pub fn appearance() -> ReadSignal<Appearance> {
        appearance()
    }
    pub fn quit() {
        window_handle().quit();
    }
    pub fn menus(mut self, menus: Vec<Menu>) -> Self {
        self.menus = menus;
        self
    }

    pub fn new() -> App {
        App::default()
    }

    pub fn fonts(mut self, fonts: impl IntoIterator<Item = Vec<u8>>) -> Self {
        self.fonts.extend(fonts);
        self
    }

    pub fn window<T: View>(
        mut self,
        options: WindowOptions,
        root: impl FnOnce() -> T + 'static,
    ) -> Self {
        self.windows
            .push((options, Box::new(move || root().into_any())));
        self
    }

    pub fn run(mut self) -> Result<(), AppError> {
        if !self.menus.iter().any(|menu| menu.title == "Edit") {
            self.menus.push(Menu::edit());
        }
        let event_loop = EventLoop::new()?;
        let proxy = event_loop.create_proxy();
        #[cfg(target_os = "macos")]
        let native_menus =
            crate::platform::native_menu::NativeMenus::new(self.menus.clone(), proxy.clone())?;
        let runtime = Runtime::with_waker(move || {
            let _ = proxy.send_event(());
        });
        let _entered = runtime.enter();
        event_loop.set_control_flow(ControlFlow::Wait);
        let mut runner = Runner {
            #[cfg(target_os = "macos")]
            native_menus,
            pending: self.windows,
            menus: self.menus,
            fonts: self.fonts,
            windows: HashMap::new(),
            error: None,
            exit_after_frames: var("ROUGE_EXIT_AFTER_FRAMES")
                .ok()
                .and_then(|v| v.parse().ok()),
        };
        event_loop.run_app(&mut runner)?;
        runner.error.map_or(Ok(()), Err)
    }
}

struct Runner {
    #[cfg(target_os = "macos")]
    native_menus: crate::platform::native_menu::NativeMenus,
    pending: Vec<(WindowOptions, RootFn)>,
    menus: Vec<Menu>,
    fonts: Vec<Vec<u8>>,
    windows: HashMap<WindowId, WindowState>,
    error: Option<AppError>,
    exit_after_frames: Option<u64>,
}

struct WindowState {
    window: Arc<Window>,
    surface: Surface<'static>,
    config: SurfaceConfiguration,
    gpu: Gpu,
    renderer: Renderer,
    ui: Ui,
    background: Color,
    frames: u64,
    occluded: bool,
    tracking: bool,
    modifiers: Modifiers,
    cursor: Cursor,
    ime: bool,
}

struct SystemClipboardAdapter(Option<SystemClipboard>);

impl Clipboard for SystemClipboardAdapter {
    fn read(&mut self) -> Option<String> {
        self.0.as_mut()?.get_text().ok()
    }

    fn write(&mut self, text: &str) {
        if let Some(clipboard) = &mut self.0 {
            let _ = clipboard.set_text(text);
        }
    }
}

impl WindowState {
    fn new(
        event_loop: &ActiveEventLoop,
        options: WindowOptions,
        root: RootFn,
        menus: Vec<Menu>,
        fonts: &[Vec<u8>],
    ) -> Result<WindowState, AppError> {
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

        let display = Box::new(event_loop.owned_display_handle());
        let instance = Instance::new(InstanceDescriptor::new_with_display_handle(display));
        let surface = instance.create_surface(window.clone())?;
        let gpu = block_on(Gpu::new(instance, Some(&surface)))?;

        let physical = window.inner_size();
        let scale = window.scale_factor() as f32;
        let formats = surface.get_capabilities(&gpu.adapter).formats;
        let linear = formats.iter().copied().find(|format| !format.is_srgb());
        let mut config = surface
            .get_default_config(&gpu.adapter, physical.width.max(1), physical.height.max(1))
            .ok_or(AppError::UnsupportedSurface)?;
        let format = linear.unwrap_or(config.format);
        config.format = format;
        config.present_mode = PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);

        let renderer = Renderer::new(&gpu.device, &gpu.queue, format);
        let chrome = options.decorations == Decorations::Client || !window.is_decorated();
        let mut ui = Ui::with_fonts(
            logical_size(physical, scale),
            fonts.iter().cloned(),
            move || {
                let handle = window_handle();
                handle.set_title(options.title);
                let root = crate::platform::menu_root(menus, root());
                if chrome {
                    window_chrome(handle.title(), root).into_any()
                } else {
                    root
                }
            },
        );
        ui.set_appearance(platform_appearance(window.theme()));
        ui.set_scale(scale);
        ui.set_clipboard(SystemClipboardAdapter(SystemClipboard::new().ok()));
        Ok(WindowState {
            window,
            surface,
            config,
            gpu,
            renderer,
            ui,
            background: options.background,
            frames: 0,
            occluded: false,
            tracking: false,
            modifiers: Modifiers::NONE,
            cursor: Cursor::Default,
            ime: false,
        })
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
        match event.state {
            ElementState::Pressed => self.ui.key_down(key_event),
            ElementState::Released => self.ui.key_up(key_event),
        };
    }

    fn sync_platform(&mut self) {
        let cursor = self.ui.cursor();
        if cursor != self.cursor {
            self.cursor = cursor;
            self.window.set_cursor(cursor_icon(cursor));
        }
        let ime = self.ui.accepts_text();
        if ime != self.ime {
            self.ime = ime;
            self.window.set_ime_allowed(ime);
        }
        if let Some(area) = self.ui.text_cursor_area().filter(|_| ime) {
            let position = LogicalPosition::new(area.origin.x, area.origin.y);
            self.window.set_ime_cursor_area(
                position,
                LogicalSize::new(area.size.width, area.size.height),
            );
        }
    }

    fn resize(&mut self, physical: PhysicalSize<u32>) {
        if physical.width == 0 || physical.height == 0 {
            return;
        }
        self.config.width = physical.width;
        self.config.height = physical.height;
        self.surface.configure(&self.gpu.device, &self.config);
        self.ui.set_size(logical_size(physical, self.ui.scale()));
        self.window.request_redraw();
    }

    fn wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase) {
        let by = scroll_delta(delta, self.ui.scale());
        let ended = matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled);
        self.tracking |= phase == TouchPhase::Started;
        match (delta, self.tracking) {
            (MouseScrollDelta::PixelDelta(_), true) => self.ui.scroll_drag(by),
            _ => { self.ui.scroll(by); }
        }
        if ended && self.tracking {
            self.tracking = false;
            self.ui.scroll_release(Point::ZERO);
        }
    }

    fn redraw(&mut self) {
        self.ui.update();
        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                frame
            }
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                self.window.request_redraw();
                return;
            }
            CurrentSurfaceTexture::Timeout => {
                self.window.request_redraw();
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let viewport = Viewport::new(self.config.width, self.config.height, self.ui.scale());
        let ui_frame = self.ui.frame();
        self.renderer.render(
            &view,
            viewport,
            self.background,
            &ui_frame.scene,
            ui_frame.atlas,
        );
        self.window.pre_present_notify();
        self.gpu.queue.present(frame);
        self.frames += 1;
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        for (options, root) in take(&mut self.pending) {
            match WindowState::new(event_loop, options, root, self.menus.clone(), &self.fonts) {
                Ok(state) => {
                    self.windows.insert(state.window.id(), state);
                }
                Err(error) => {
                    self.error = Some(error);
                    event_loop.exit();
                    return;
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        for state in self.windows.values_mut() {
            if !state.occluded && state.ui.needs_frame() {
                state.window.request_redraw();
            }
        }
        self.process_window_commands(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, (): ()) {
        #[cfg(target_os = "macos")]
        {
            let ui = self
                .windows
                .values_mut()
                .find(|state| state.window.has_focus())
                .map(|state| &mut state.ui);
            if self.native_menus.dispatch(ui) {
                self.windows.clear();
                event_loop.exit();
            }
        }
        self.process_window_commands(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(state) = self.windows.get_mut(&id) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                self.windows.remove(&id);
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }
            WindowEvent::ThemeChanged(theme) => {
                state.ui.set_appearance(platform_appearance(Some(theme)));
            }
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                state.ui.set_scale(scale_factor as f32);
                state.resize(state.window.inner_size());
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = logical_point(position, state.ui.scale());
                state.ui.pointer_moved(position);
            }
            WindowEvent::CursorLeft { .. } => state.ui.pointer_left(),
            WindowEvent::MouseInput {
                state: pressed,
                button,
                ..
            } => match pressed {
                ElementState::Pressed => state.ui.pointer_down(pointer_button(button)),
                ElementState::Released => state.ui.pointer_up(pointer_button(button)),
            },
            WindowEvent::MouseWheel { delta, phase, .. } => state.wheel(delta, phase),
            WindowEvent::ModifiersChanged(changed) => {
                state.modifiers = modifiers(changed.state());
                state.ui.set_modifiers(state.modifiers);
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => state.key(event),
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                state.ui.ime(ImeEvent::Preedit { text, cursor });
            }
            WindowEvent::Ime(Ime::Commit(text)) => state.ui.ime(ImeEvent::Commit(text)),
            WindowEvent::Occluded(occluded) => {
                state.occluded = occluded;
                if !occluded {
                    state.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if !state.occluded {
                    state.redraw();
                }
                if self.exit_after_frames.is_some_and(|n| state.frames >= n) {
                    self.windows.clear();
                    event_loop.exit();
                }
            }
            _ => {}
        }
        if let Some(state) = self.windows.get_mut(&id) {
            state.sync_platform();
        }
        self.process_window_commands(event_loop);
    }
}

impl Runner {
    fn process_window_commands(&mut self, event_loop: &ActiveEventLoop) {
        let mut closing = Vec::new();
        let mut quit = false;
        for (id, state) in &mut self.windows {
            let (close, exit) = window_commands(&state.window, &mut state.ui);
            if close {
                closing.push(*id);
            }
            quit |= exit;
        }
        for id in closing {
            self.windows.remove(&id);
        }
        if quit || self.windows.is_empty() {
            self.windows.clear();
            event_loop.exit();
        }
    }
}
