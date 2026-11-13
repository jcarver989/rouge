#[cfg(target_os = "macos")]
pub(crate) mod native_menu;

use rouge_core::{
    Appearance, Cursor, Key, Modifiers, PointerButton, ResizeEdge, Ui, WindowCommand,
};
use rouge_render::{Point, Size};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{MouseButton, MouseScrollDelta};
use winit::keyboard::{ModifiersState, NamedKey};
use winit::window::{CursorIcon, ResizeDirection, Theme, Window};

const LINE_SCROLL: f32 = 40.0;

fn logical<T: Into<f64>>(x: T, y: T, scale: f32) -> (f32, f32) {
    (x.into() as f32 / scale, y.into() as f32 / scale)
}

pub fn logical_point(physical: PhysicalPosition<f64>, scale: f32) -> Point {
    let (x, y) = logical(physical.x, physical.y, scale);
    Point::new(x, y)
}

pub fn logical_size(physical: PhysicalSize<u32>, scale: f32) -> Size {
    let (width, height) = logical(physical.width, physical.height, scale);
    Size::new(width, height)
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
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::ResizeHorizontal => CursorIcon::EwResize,
        Cursor::ResizeVertical => CursorIcon::NsResize,
    }
}

pub fn scroll_delta(delta: MouseScrollDelta, scale: f32) -> Point {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => Point::new(x * LINE_SCROLL, y * LINE_SCROLL),
        MouseScrollDelta::PixelDelta(delta) => logical_point(delta, scale),
    }
}

pub fn appearance(theme: Option<Theme>) -> Appearance {
    if theme == Some(Theme::Dark) {
        Appearance::Dark
    } else {
        Appearance::Light
    }
}

pub(crate) fn window_commands(window: &Window, ui: &mut Ui) -> (bool, bool) {
    let (mut close, mut quit) = (false, false);
    loop {
        let commands = ui.take_window_commands();
        if commands.is_empty() {
            break;
        }
        for command in commands {
            match command {
                WindowCommand::SetTitle(title) => window.set_title(&title),
                WindowCommand::Minimize => window.set_minimized(true),
                WindowCommand::Zoom => window.set_maximized(!window.is_maximized()),
                WindowCommand::Close => close = true,
                WindowCommand::Quit => quit = true,
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
                WindowCommand::Menu(item) => {
                    item.dispatch(ui);
                }
            }
        }
    }
    (close, quit)
}

#[cfg(feature = "open-url")]
pub fn open_url(url: &str) -> Result<(), std::io::Error> {
    open::that(url)
}

pub(crate) fn menu_root(
    menus: Vec<rouge_core::Menu>,
    root: rouge_core::AnyView,
) -> rouge_core::AnyView {
    #[cfg(target_os = "macos")]
    {
        let _ = menus;
        root
    }
    #[cfg(not(target_os = "macos"))]
    {
        use rouge_core::{Styled, View, div};
        if menus.is_empty() {
            return root;
        }
        div()
            .size_full()
            .flex()
            .col()
            .child(rouge_widgets::menu_bar(menus))
            .child(div().flex_1().min_h(0.0).root_host().child(root))
            .into_any()
    }
}
