use super::{AndroidClipboard, Insets, appearance, insets, leave, refresh_rate};
use crate::app::{App, AppError, RootFn, WindowOptions};
use crate::compositing::Compositing;
use crate::driver::{Driver, Setup, connect};
use crate::platform::editing::{Editing, Previous, selected};
use crate::platform::{PrimaryTouch, default_menus, warn, web_engine, window_root};
use crate::profile::{ProfileSettings, trace};
use accesskit_android::InjectingAdapter;
use accesskit_android::jni::JavaVM;
use accesskit_android::jni::objects::JObject;
use android_activity::input::{
    ImeOptions, InputEvent, InputType, KeyAction, KeyEvent as AndroidKey, KeyMapChar, Keycode,
    MetaState, MotionAction, MotionEvent, TextInputAction, TextInputState, TextSpan,
};
use android_activity::ndk::native_window::NativeWindow;
use android_activity::{AndroidApp, AndroidAppWaker, InputStatus, MainEvent, PollEvent};
use pollster::block_on;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use rouge_core::access::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use rouge_core::text::{byte_offset, utf16_offset};
use rouge_core::{
    CompositorEvent, ContentType, EditTransaction, EnterKey, InputKind, Key, KeyEvent, Menu,
    Modifiers, TouchPhase, WindowCommand, WindowId,
};
use rouge_reactive::Runtime;
#[cfg(feature = "tokio-rt")]
use rouge_reactive::set_tokio;
use rouge_render::{Edges, Gpu, Point};
use std::borrow::Cow;
use std::env::var;
use std::mem::take;
use std::ops::Range;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use tracing_chrome::FlushGuard;
use wgpu::{Instance, InstanceDescriptor};

const DENSITY_BASELINE: f32 = 160.0;
const MIRROR_LIMIT: usize = (1 << 16) - 1;
const REJECTED: &str = "\0";

pub(super) enum Exit {
    Destroyed,
    Quit,
}

pub(super) fn run(app: App, android: AndroidApp) -> Result<Exit, AppError> {
    let waker = android.create_waker();
    let wake = move || waker.wake();
    let runtime = Runtime::with_waker(wake);
    let _entered = runtime.enter();
    #[cfg(feature = "tokio-rt")]
    if let Some(handle) = app.tokio {
        set_tokio(handle);
    }
    let profile = ProfileSettings::read(|name| var(name.to_ascii_uppercase()).ok());
    let (assist, requests) = channel();
    let mut host = Host {
        android: android.clone(),
        _trace: trace(&profile),
        gpu: None,
        compositor: None,
        pending: app.windows.into_iter().next(),
        menus: default_menus(app.menus),
        fonts: app.fonts,
        profile,
        window: None,
        assist,
        requests,
        deadline: None,
        exit: None,
    };
    let exit = loop {
        let timeout = host
            .deadline
            .map(|due| due.saturating_duration_since(Instant::now()));
        android.poll_events(timeout, |event| {
            if let PollEvent::Main(event) = event {
                host.lifecycle(&event);
            }
        });
        host.input();
        host.assist();
        host.turn();
        if let Some(exit) = host.exit.take() {
            break exit;
        }
    };
    let closed = host.compositor.take().map_or(Ok(()), Compositing::close);
    host.window = None;
    let exit = exit?;
    closed?;
    Ok(exit)
}

#[derive(Debug)]
struct Display;

impl HasDisplayHandle for Display {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::android())
    }
}

struct SurfaceWindow(NativeWindow);

impl HasWindowHandle for SurfaceWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for SurfaceWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::android())
    }
}

enum Assist {
    Tree,
    Action(ActionRequest),
}

struct Requests(Sender<Assist>, AndroidAppWaker);

impl Requests {
    fn send(&self, request: Assist) {
        let _ = self.0.send(request);
        self.1.wake();
    }
}

impl ActivationHandler for Requests {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.send(Assist::Tree);
        None
    }
}

impl ActionHandler for Requests {
    fn do_action(&mut self, request: ActionRequest) {
        self.send(Assist::Action(request));
    }
}

fn state(editing: &Editing) -> TextInputState {
    let unit = |byte| utf16_offset(&editing.text, byte);
    let span = |range: &Range<usize>| TextSpan {
        start: unit(range.start),
        end: unit(range.end),
    };
    TextInputState {
        text: editing.text.clone(),
        selection: span(&editing.selection),
        compose_region: editing.marked.as_ref().map(span),
    }
}

fn blank() -> TextInputState {
    TextInputState {
        text: String::new(),
        selection: TextSpan { start: 0, end: 0 },
        compose_region: None,
    }
}

fn fits(text: &str) -> bool {
    let unit = |character: char| match character.len_utf8() {
        _ if character == '\0' => 2,
        4 => 6,
        length => length,
    };
    text.chars().map(unit).sum::<usize>() <= MIRROR_LIMIT
}

fn editor_info(editing: &Editing, mirrored: bool) -> (InputType, TextInputAction, ImeOptions) {
    let Editing {
        hints, multiline, ..
    } = editing;
    let mut kind = match hints.content {
        _ if !mirrored => InputType::TYPE_NULL,
        ContentType::Text | ContentType::Search => InputType::TYPE_CLASS_TEXT,
        ContentType::Email => {
            InputType::TYPE_CLASS_TEXT | InputType::TYPE_TEXT_VARIATION_EMAIL_ADDRESS
        }
        ContentType::Url => InputType::TYPE_CLASS_TEXT | InputType::TYPE_TEXT_VARIATION_URI,
        ContentType::Number => InputType::TYPE_CLASS_NUMBER | InputType::TYPE_NUMBER_FLAG_DECIMAL,
        ContentType::Phone => InputType::TYPE_CLASS_PHONE,
    };
    if mirrored && !matches!(hints.content, ContentType::Number | ContentType::Phone) {
        kind |= if hints.autocorrect {
            InputType::TYPE_TEXT_FLAG_AUTO_CORRECT
        } else {
            InputType::TYPE_TEXT_FLAG_NO_SUGGESTIONS
        };
        if *multiline {
            kind |= InputType::TYPE_TEXT_FLAG_MULTI_LINE;
        }
    }
    let action = match hints.enter {
        EnterKey::Default if *multiline => TextInputAction::None,
        EnterKey::Default => TextInputAction::Unspecified,
        EnterKey::Done => TextInputAction::Done,
        EnterKey::Go => TextInputAction::Go,
        EnterKey::Next => TextInputAction::Next,
        EnterKey::Search => TextInputAction::Search,
        EnterKey::Send => TextInputAction::Send,
    };
    let options = ImeOptions::IME_FLAG_NO_FULLSCREEN
        | ImeOptions::from_bits_retain(i32::from(action).cast_unsigned());
    (kind, action, options)
}

struct Window {
    driver: Driver,
    access: InjectingAdapter,
    occluded: bool,
    touch: PrimaryTouch,
    insets: Insets,
    known: Option<Editing>,
    mirrored: bool,
    tapped: bool,
}

impl Window {
    fn step(&mut self) {
        let update = self.driver.step();
        self.publish(update);
    }

    fn publish(&mut self, update: Option<TreeUpdate>) {
        if let Some(update) = update {
            self.access.update_if_active(|| update);
        }
    }

    fn resize(&mut self, android: &AndroidApp) {
        let Some(native) = android.native_window() else {
            return;
        };
        let resized = self.driver.resize(pixels(&native), scale(android));
        if resized.is_some() && !self.occluded {
            self.step();
        }
    }

    fn sync_environment(&mut self, android: &AndroidApp) {
        let scale = scale(android);
        let Insets { safe, keyboard } = self.insets;
        let ui = self.driver.ui();
        ui.set_appearance(appearance());
        ui.set_safe_area(Edges {
            top: safe.top / scale,
            right: safe.right / scale,
            bottom: safe.bottom / scale,
            left: safe.left / scale,
        });
        ui.set_keyboard_inset(keyboard / scale);
    }

    fn sync_text(&mut self, android: &AndroidApp) {
        let editor = self.driver.ui().editor();
        if take(&mut self.tapped) && editor.is_some() {
            android.show_soft_input(false);
        }
        let Some(Previous(previous)) = Editing::track(&mut self.known, editor.as_ref()) else {
            return;
        };
        let Some(editing) = &self.known else {
            return android.hide_soft_input(false);
        };
        let mirrored = fits(&editing.text);
        let described = mirrored == self.mirrored
            && previous.as_ref().is_some_and(|previous| {
                (previous.hints, previous.multiline) == (editing.hints, editing.multiline)
            });
        self.mirrored = mirrored;
        if !described {
            let (kind, action, options) = editor_info(editing, mirrored);
            android.set_ime_editor_info(kind, action, options);
        }
        if mirrored {
            android.set_text_input_state(state(editing));
        } else if !described {
            android.set_text_input_state(blank());
        }
        if previous.is_none() {
            android.show_soft_input(false);
        }
    }

    fn text(&mut self, android: &AndroidApp, state: &TextInputState) {
        if state.text == REJECTED {
            warn("an input method edit exceeded the 64 KiB text buffer and was dropped");
            self.known = None;
            return;
        }
        let ui = self.driver.ui();
        let (Some(known), Some(editor)) = (&self.known, ui.editor()) else {
            return;
        };
        if !known.holds(&editor) {
            return;
        }
        let text = &state.text;
        let range = |span: TextSpan| {
            let byte = |unit| byte_offset(text, unit);
            byte(span.start.min(span.end))..byte(span.start.max(span.end))
        };
        let marked = (state.compose_region.map(range)).filter(|range| !range.is_empty());
        if !self.mirrored {
            if !text.is_empty() && marked.is_none() {
                let typed = EditTransaction::new().replace(selected(&editor), text.as_str());
                ui.edit_text(typed);
                android.set_text_input_state(blank());
            }
            return;
        }
        let incoming = Editing {
            text: text.clone(),
            selection: range(state.selection),
            marked,
            id: known.id,
            multiline: known.multiline,
            hints: known.hints,
        };
        if incoming == *known {
            return;
        }
        let mut transaction = EditTransaction::between(&known.text, text)
            .select(incoming.selection.start, incoming.selection.end);
        if let Some(marked) = incoming.marked.clone() {
            transaction = transaction.mark(marked);
        }
        self.known = Some(incoming);
        ui.edit_text(transaction);
    }

    fn action(&mut self, action: TextInputAction) {
        let ui = self.driver.ui();
        match action {
            TextInputAction::Next | TextInputAction::Previous => {
                ui.focus_next(action == TextInputAction::Previous);
            }
            _ => {
                ui.key_down(KeyEvent::new(Key::Enter));
                ui.key_up(KeyEvent::new(Key::Enter));
            }
        }
    }

    fn touch(&mut self, event: &MotionEvent<'_>) {
        let phase = match event.action() {
            MotionAction::Down | MotionAction::PointerDown => TouchPhase::Start,
            MotionAction::Up | MotionAction::PointerUp => TouchPhase::End,
            MotionAction::Move => TouchPhase::Move,
            MotionAction::Cancel => TouchPhase::Cancel,
            _ => return,
        };
        let single = matches!(phase, TouchPhase::Start | TouchPhase::End);
        let ui = self.driver.ui();
        let scale = ui.scale();
        for (index, pointer) in event.pointers().enumerate() {
            if single && index != event.pointer_index() {
                continue;
            }
            let id = u64::from(pointer.pointer_id().cast_unsigned());
            if let Some(phase) = self.touch.input(id, phase) {
                ui.touch(phase, Point::new(pointer.x() / scale, pointer.y() / scale));
                self.tapped |= phase == TouchPhase::End;
            }
        }
    }

    fn key(&mut self, android: &AndroidApp, event: &AndroidKey<'_>) -> InputStatus {
        let (code, meta) = (event.key_code(), event.meta_state());
        let typed = android
            .device_key_character_map(event.device_id())
            .and_then(|map| map.get(code, meta))
            .ok()
            .and_then(|mapped| match mapped {
                KeyMapChar::Unicode(character) => Some(character.to_string()),
                _ => None,
            });
        let character = typed.as_deref().map(Key::character);
        let Some(key) = named_key(code).or(character) else {
            return InputStatus::Unhandled;
        };
        let mut key_event = KeyEvent::new(key).with_modifiers(modifiers(meta));
        key_event.repeat = event.repeat_count() > 0;
        let ui = self.driver.ui();
        if event.action() == KeyAction::Down {
            key_event.text = typed;
            if !ui.key_down(key_event) && code == Keycode::Back {
                leave();
            }
        } else {
            ui.key_up(key_event);
        }
        InputStatus::Handled
    }
}

struct Host {
    android: AndroidApp,
    _trace: Option<FlushGuard>,
    gpu: Option<(Instance, Gpu)>,
    compositor: Option<Compositing>,
    pending: Option<(WindowOptions, RootFn)>,
    menus: Vec<Menu>,
    fonts: Vec<Cow<'static, [u8]>>,
    profile: ProfileSettings,
    window: Option<Window>,
    assist: Sender<Assist>,
    requests: Receiver<Assist>,
    deadline: Option<Instant>,
    exit: Option<Result<Exit, AppError>>,
}

impl Host {
    fn lifecycle(&mut self, event: &MainEvent<'_>) {
        match event {
            MainEvent::InitWindow { .. } => {
                if let Err(error) = self.surface() {
                    self.exit = Some(Err(error));
                }
            }
            MainEvent::TerminateWindow { .. } => self.detach(),
            MainEvent::WindowResized { .. }
            | MainEvent::ContentRectChanged { .. }
            | MainEvent::ConfigChanged { .. }
            | MainEvent::RedrawNeeded { .. } => {
                if let Some(window) = &mut self.window {
                    window.resize(&self.android);
                }
            }
            MainEvent::InsetsChanged { .. } => {
                if let (Some(window), Some(insets)) = (&mut self.window, insets()) {
                    window.insets = insets;
                }
            }
            MainEvent::GainedFocus => self.focused(true),
            MainEvent::LostFocus => self.focused(false),
            MainEvent::Destroy => {
                self.exit.get_or_insert(Ok(Exit::Destroyed));
            }
            _ => {}
        }
    }

    fn focused(&mut self, focused: bool) {
        if let Some(window) = &mut self.window {
            window.driver.ui().set_focused(focused);
        }
    }

    fn surface(&mut self) -> Result<(), AppError> {
        let Some(native) = self.android.native_window() else {
            return Ok(());
        };
        let (pixels, scale) = (pixels(&native), scale(&self.android));
        if let (Some(window), Some((instance, gpu))) = (&mut self.window, &self.gpu) {
            let surface = instance.create_surface(SurfaceWindow(native))?;
            window.driver.resize(pixels, scale);
            let target = window.driver.target(gpu, surface)?;
            window.driver.reattach(target);
            window.occluded = false;
            window.driver.ui().set_visible(true);
            window.step();
            return Ok(());
        }
        let Some((options, root)) = self.pending.take() else {
            return Ok(());
        };
        let instance = async {
            Instance::new(InstanceDescriptor::new_with_display_handle_from_env(
                Box::new(Display),
            ))
        };
        let (gpu, surface) = block_on(connect(&mut self.gpu, instance, SurfaceWindow(native)))?;
        let compositor = match &self.compositor {
            Some(compositor) => compositor,
            None => {
                let waker = self.android.create_waker();
                (self.compositor).insert(Compositing::open(&gpu, move || waker.wake())?)
            }
        };
        let setup = Setup {
            pixels,
            scale,
            interval: refresh_interval(),
            epoch: compositor.epoch,
            appearance: appearance(),
            input: InputKind::Touch,
            menus: &self.menus,
            fonts: &self.fonts,
            profile: &self.profile,
        };
        let title = options.title;
        let engine = web_engine(self.android.clone());
        let (driver, target) = Driver::open(
            WindowId(0),
            &gpu,
            surface,
            setup,
            compositor.messages.clone(),
            AndroidClipboard,
            move || window_root(title, Vec::new(), false, root, engine),
        )?;
        driver.attach(target);
        let mut window = Window {
            driver,
            access: self.access(),
            occluded: false,
            touch: PrimaryTouch::default(),
            insets: insets().unwrap_or_default(),
            known: None,
            mirrored: true,
            tapped: false,
        };
        window.sync_environment(&self.android);
        self.window = Some(window);
        Ok(())
    }

    #[allow(unsafe_code)]
    fn access(&self) -> InjectingAdapter {
        let vm = unsafe { JavaVM::from_raw(self.android.vm_as_ptr().cast()) }
            .expect("android-activity holds a live Java VM");
        let mut env = vm
            .attach_current_thread_permanently()
            .expect("the android_main thread attaches to the Java VM");
        let activity = unsafe { JObject::from_raw(self.android.activity_as_ptr().cast()) };
        let view = env
            .get_field(
                &activity,
                "mSurfaceView",
                "Lcom/google/androidgamesdk/GameActivity$InputEnabledSurfaceView;",
            )
            .expect("GameActivity declares its surface view")
            .l()
            .expect("GameActivity owns an input-enabled surface view");
        let requests = || Requests(self.assist.clone(), self.android.create_waker());
        InjectingAdapter::new(&mut env, &view, requests(), requests())
    }

    fn detach(&mut self) {
        let (Some(window), Some(compositor)) = (&mut self.window, &mut self.compositor) else {
            return;
        };
        window.occluded = true;
        let ui = window.driver.ui();
        ui.touch(TouchPhase::Cancel, Point::ZERO);
        ui.set_visible(false);
        window.touch = PrimaryTouch::default();
        let detached = window.driver.detach();
        compositor.turn();
        let _ = detached.recv();
    }

    fn input(&mut self) {
        let Some(window) = &mut self.window else {
            return;
        };
        let Ok(mut events) = self.android.input_events_iter() else {
            return;
        };
        let android = &self.android;
        while events.next(|event| match event {
            InputEvent::MotionEvent(event) => {
                window.touch(event);
                InputStatus::Handled
            }
            InputEvent::KeyEvent(event) => window.key(android, event),
            InputEvent::TextEvent(state) => {
                window.text(android, state);
                InputStatus::Handled
            }
            InputEvent::TextAction(action) => {
                window.action(*action);
                InputStatus::Handled
            }
            _ => InputStatus::Unhandled,
        }) {}
    }

    fn assist(&mut self) {
        let Some(window) = &mut self.window else {
            return;
        };
        let mut changed = false;
        for request in self.requests.try_iter() {
            changed = true;
            let ui = window.driver.ui();
            match request {
                Assist::Tree => window.access.update_if_active(|| ui.accessibility_tree()),
                Assist::Action(request) => {
                    ui.accessibility_action(request);
                }
            }
        }
        if changed && window.occluded {
            let update = window.driver.refresh();
            window.publish(update);
        }
    }

    fn turn(&mut self) {
        let Some(window) = &mut self.window else {
            self.deadline = None;
            return;
        };
        window.sync_environment(&self.android);
        pump(&mut self.compositor, &mut window.driver);
        let now = Instant::now();
        if !window.occluded && window.driver.frame_due(now).is_some_and(|due| due <= now) {
            window.step();
        }
        let mut deadline = pump(&mut self.compositor, &mut window.driver);
        if !window.occluded {
            let due = window.driver.frame_due(Instant::now());
            deadline = [deadline, due].into_iter().flatten().min();
        }
        window.sync_text(&self.android);
        let mut commands = Vec::new();
        window.driver.commands(&mut commands);
        let closing = commands
            .iter()
            .any(|command| matches!(command, WindowCommand::Close | WindowCommand::Quit));
        if closing || window.driver.finished() {
            window.driver.close();
            self.exit.get_or_insert(Ok(Exit::Quit));
        }
        self.deadline = deadline;
    }
}

fn pump(compositor: &mut Option<Compositing>, driver: &mut Driver) -> Option<Instant> {
    let compositor = compositor.as_mut()?;
    let deadline = compositor.turn();
    for (_, event) in compositor.events.try_iter() {
        if event != CompositorEvent::Closed {
            driver.handle(event);
        }
    }
    deadline
}

fn pixels(native: &NativeWindow) -> (u32, u32) {
    (
        native.width().cast_unsigned(),
        native.height().cast_unsigned(),
    )
}

fn scale(android: &AndroidApp) -> f32 {
    android
        .config()
        .density()
        .map_or(1.0, |dpi| dpi as f32 / DENSITY_BASELINE)
}

fn refresh_interval() -> Duration {
    let hertz = refresh_rate().filter(|rate| *rate > 0.0).unwrap_or(60.0);
    Duration::from_secs_f64(1.0 / f64::from(hertz))
}

fn modifiers(meta: MetaState) -> Modifiers {
    Modifiers {
        shift: meta.shift_on(),
        ctrl: meta.ctrl_on(),
        alt: meta.alt_on(),
        meta: meta.meta_on(),
    }
}

fn named_key(code: Keycode) -> Option<Key> {
    Some(match code {
        Keycode::Back | Keycode::Escape => Key::Escape,
        Keycode::Enter | Keycode::NumpadEnter => Key::Enter,
        Keycode::Tab => Key::Tab,
        Keycode::Space => Key::Space,
        Keycode::Del => Key::Backspace,
        Keycode::ForwardDel => Key::Delete,
        Keycode::DpadLeft => Key::ArrowLeft,
        Keycode::DpadRight => Key::ArrowRight,
        Keycode::DpadUp => Key::ArrowUp,
        Keycode::DpadDown => Key::ArrowDown,
        Keycode::MoveHome => Key::Home,
        Keycode::MoveEnd => Key::End,
        Keycode::PageUp => Key::PageUp,
        Keycode::PageDown => Key::PageDown,
        _ => {
            let number = u32::from(code).checked_sub(u32::from(Keycode::F1))?;
            return u8::try_from(number + 1)
                .ok()
                .filter(|number| *number <= 12)
                .map(Key::F);
        }
    })
}
