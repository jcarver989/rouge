#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod clipboard;
mod runner;

pub(crate) use runner::run;

use super::{LINE_SCROLL, Platform, css_cursor};
use crate::app::AppError;
use rouge_core::{
    Appearance, Clipboard, Cursor, Key, Menu, Modifiers, PointerButton, ResizeEdge,
    TouchPhase as UiTouchPhase, Ui, WindowCommand,
};
use rouge_render::{Edges, Point, Size};
#[cfg(feature = "persist")]
use std::env::var_os;
#[cfg(feature = "persist")]
use std::path::PathBuf;
use std::time::{Duration, Instant};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{KeyEvent, MouseButton, MouseScrollDelta, TouchPhase};
use winit::event_loop::{EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Icon, ResizeDirection, Theme, Window};

pub(crate) type UserEvent = Option<accesskit_winit::Event>;

pub(crate) trait WinitPlatform: Platform + Sized {
    const SURFACE_LOST_ON_SUSPEND: bool = false;

    fn new(menus: &[Menu], waker: &EventLoopProxy<UserEvent>) -> Result<Self, AppError>;

    fn clipboard() -> impl Clipboard + 'static;

    fn app_icon(_png: &[u8]) -> Option<Icon> {
        None
    }

    #[cfg(feature = "persist")]
    fn default_config_dir(name: &str) -> Option<PathBuf>;

    #[cfg(feature = "persist")]
    fn config_dir(name: &str) -> Option<PathBuf> {
        var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join(name.to_lowercase()))
            .or_else(|| Self::default_config_dir(name))
    }

    fn event_loop(_options: Self::Options) -> Result<EventLoop<UserEvent>, AppError> {
        Ok(EventLoop::with_user_event().build()?)
    }

    fn surface_size(window: &Window) -> PhysicalSize<u32> {
        window.inner_size()
    }

    fn safe_area(_window: &Window) -> Edges {
        Edges::ZERO
    }

    fn keyboard_inset(_window: &Window) -> f32 {
        0.0
    }

    fn appearance(window: &Window) -> Appearance {
        appearance(window.theme())
    }

    fn refresh_millihertz(window: &Window) -> Option<u32> {
        window
            .current_monitor()
            .and_then(|monitor| monitor.refresh_rate_millihertz())
    }

    fn key(event: &KeyEvent) -> WinitKey {
        event.logical_key.clone()
    }

    fn unhandled_key(_event: &KeyEvent) {}

    fn sync(_window: &Window, _ui: &mut Ui) {}

    fn menu_events(&mut self, _ui: Option<&mut Ui>) -> bool {
        false
    }

    fn exit() {}
}

const MOMENTUM_GAP: Duration = Duration::from_millis(50);

pub fn logical_point(physical: PhysicalPosition<f64>, scale: f32) -> Point {
    Point::new(physical.x as f32 / scale, physical.y as f32 / scale)
}

pub fn logical_size(physical: PhysicalSize<u32>, scale: f32) -> Size {
    Size::new(
        physical.width as f32 / scale,
        physical.height as f32 / scale,
    )
}

pub fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Other(3),
        MouseButton::Forward => PointerButton::Other(4),
        MouseButton::Other(n) => PointerButton::Other(n),
    }
}

pub fn named_key(key: NamedKey) -> Key {
    match key {
        NamedKey::Enter => Key::Enter,
        NamedKey::Tab => Key::Tab,
        NamedKey::Space => Key::Space,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::Escape => Key::Escape,
        NamedKey::ArrowLeft => Key::ArrowLeft,
        NamedKey::ArrowRight => Key::ArrowRight,
        NamedKey::ArrowUp => Key::ArrowUp,
        NamedKey::ArrowDown => Key::ArrowDown,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::F1 => Key::F(1),
        NamedKey::F2 => Key::F(2),
        NamedKey::F3 => Key::F(3),
        NamedKey::F4 => Key::F(4),
        NamedKey::F5 => Key::F(5),
        NamedKey::F6 => Key::F(6),
        NamedKey::F7 => Key::F(7),
        NamedKey::F8 => Key::F(8),
        NamedKey::F9 => Key::F(9),
        NamedKey::F10 => Key::F(10),
        NamedKey::F11 => Key::F(11),
        NamedKey::F12 => Key::F(12),
        _ => Key::Other,
    }
}

pub fn modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        meta: state.super_key(),
    }
}

pub fn cursor_icon(cursor: Cursor) -> CursorIcon {
    css_cursor(cursor).parse().unwrap_or_default()
}

pub fn scroll_delta(delta: MouseScrollDelta, scale: f32) -> Point {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => Point::new(x * LINE_SCROLL, y * LINE_SCROLL),
        MouseScrollDelta::PixelDelta(delta) => logical_point(delta, scale),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelGesture {
    Scroll(Point),
    Drag(Point),
    Release(Point),
    Coast,
}

#[derive(Clone, Copy, Debug, Default)]
enum WheelPhase {
    #[default]
    Idle,
    Tracking,
    Lifted(Instant),
    Coasting,
}

#[derive(Debug, Default)]
pub struct Wheel(WheelPhase);

impl Wheel {
    pub fn input(
        &mut self,
        delta: MouseScrollDelta,
        phase: TouchPhase,
        scale: f32,
        now: Instant,
    ) -> WheelGesture {
        let by = scroll_delta(delta, scale);
        if let MouseScrollDelta::LineDelta(..) = delta {
            return WheelGesture::Scroll(by);
        }
        let ended = matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled);
        let (next, gesture) = match (self.0, phase) {
            (WheelPhase::Lifted(at), TouchPhase::Started)
                if now.saturating_duration_since(at) <= MOMENTUM_GAP =>
            {
                (WheelPhase::Coasting, WheelGesture::Coast)
            }
            (_, TouchPhase::Started) => (WheelPhase::Tracking, WheelGesture::Drag(by)),
            (WheelPhase::Tracking, _) if ended => {
                (WheelPhase::Lifted(now), WheelGesture::Release(by))
            }
            (WheelPhase::Tracking, _) => (WheelPhase::Tracking, WheelGesture::Drag(by)),
            (WheelPhase::Coasting, _) if ended => (WheelPhase::Idle, WheelGesture::Coast),
            (WheelPhase::Coasting, _) => (WheelPhase::Coasting, WheelGesture::Coast),
            (phase, _) => (phase, WheelGesture::Scroll(by)),
        };
        self.0 = next;
        gesture
    }
}

#[derive(Debug, Default)]
pub struct PrimaryTouch(Option<u64>);

impl PrimaryTouch {
    pub fn input(&mut self, id: u64, phase: TouchPhase) -> Option<UiTouchPhase> {
        match phase {
            TouchPhase::Started if self.0.is_none() => {
                self.0 = Some(id);
                Some(UiTouchPhase::Start)
            }
            TouchPhase::Moved if self.0 == Some(id) => Some(UiTouchPhase::Move),
            TouchPhase::Ended if self.0 == Some(id) => {
                self.0 = None;
                Some(UiTouchPhase::End)
            }
            TouchPhase::Cancelled if self.0 == Some(id) => {
                self.0 = None;
                Some(UiTouchPhase::Cancel)
            }
            _ => None,
        }
    }
}

pub fn appearance(theme: Option<Theme>) -> Appearance {
    if theme == Some(Theme::Dark) {
        Appearance::Dark
    } else {
        Appearance::Light
    }
}

pub(crate) fn window_command(window: &Window, command: WindowCommand) {
    match command {
        WindowCommand::SetTitle(title) => window.set_title(&title),
        WindowCommand::Minimize => window.set_minimized(true),
        WindowCommand::Zoom => window.set_maximized(!window.is_maximized()),
        WindowCommand::Focus => window.focus_window(),
        WindowCommand::StartDrag => {
            let _ = window.drag_window();
        }
        WindowCommand::StartResize(edge) => {
            let direction = match edge {
                ResizeEdge::North => ResizeDirection::North,
                ResizeEdge::South => ResizeDirection::South,
                ResizeEdge::East => ResizeDirection::East,
                ResizeEdge::West => ResizeDirection::West,
                ResizeEdge::NorthEast => ResizeDirection::NorthEast,
                ResizeEdge::NorthWest => ResizeDirection::NorthWest,
                ResizeEdge::SouthEast => ResizeDirection::SouthEast,
                ResizeEdge::SouthWest => ResizeDirection::SouthWest,
            };
            let _ = window.drag_resize_window(direction);
        }
        WindowCommand::Close
        | WindowCommand::Quit
        | WindowCommand::Menu(_)
        | WindowCommand::Navigate(_) => {}
    }
}
