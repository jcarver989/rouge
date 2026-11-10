use rouge::core::{Cursor, Key, Modifiers, Point, PointerButton, Size};
use rouge::platform::{
    cursor_icon, logical_point, logical_size, modifiers, named_key, pointer_button, scroll_delta,
};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{MouseButton, MouseScrollDelta};
use winit::keyboard::{ModifiersState, NamedKey};
use winit::window::CursorIcon;

#[test]
fn named_keys_map_to_rouge_keys_and_unknown_ones_to_other() {
    for (named, key) in [
        (NamedKey::Enter, Key::Enter),
        (NamedKey::Tab, Key::Tab),
        (NamedKey::Space, Key::Space),
        (NamedKey::Backspace, Key::Backspace),
        (NamedKey::Delete, Key::Delete),
        (NamedKey::Escape, Key::Escape),
        (NamedKey::ArrowLeft, Key::ArrowLeft),
        (NamedKey::ArrowRight, Key::ArrowRight),
        (NamedKey::ArrowUp, Key::ArrowUp),
        (NamedKey::ArrowDown, Key::ArrowDown),
        (NamedKey::Home, Key::Home),
        (NamedKey::End, Key::End),
        (NamedKey::PageUp, Key::PageUp),
        (NamedKey::PageDown, Key::PageDown),
        (NamedKey::F1, Key::F(1)),
        (NamedKey::F12, Key::F(12)),
        (NamedKey::CapsLock, Key::Other),
    ] {
        assert_eq!(named_key(named), key, "{named:?}");
    }
}

#[test]
fn modifier_state_maps_each_flag_independently() {
    for (state, expected) in [
        (ModifiersState::empty(), Modifiers::NONE),
        (ModifiersState::SHIFT, Modifiers::SHIFT),
        (ModifiersState::CONTROL, Modifiers::CTRL),
        (ModifiersState::ALT, Modifiers::ALT),
        (ModifiersState::SUPER, Modifiers::META),
    ] {
        assert_eq!(modifiers(state), expected, "{state:?}");
    }
    let both = modifiers(ModifiersState::SHIFT | ModifiersState::SUPER);
    assert!(both.shift && both.meta && !both.ctrl && !both.alt);
}

#[test]
fn cursors_and_pointer_buttons_map_to_their_platform_counterparts() {
    for (cursor, icon) in [
        (Cursor::Default, CursorIcon::Default),
        (Cursor::Pointer, CursorIcon::Pointer),
        (Cursor::Text, CursorIcon::Text),
        (Cursor::Grab, CursorIcon::Grab),
        (Cursor::Grabbing, CursorIcon::Grabbing),
        (Cursor::NotAllowed, CursorIcon::NotAllowed),
        (Cursor::Crosshair, CursorIcon::Crosshair),
        (Cursor::ResizeHorizontal, CursorIcon::EwResize),
        (Cursor::ResizeVertical, CursorIcon::NsResize),
    ] {
        assert_eq!(cursor_icon(cursor), icon, "{cursor:?}");
    }
    for (button, expected) in [
        (MouseButton::Left, PointerButton::Primary),
        (MouseButton::Right, PointerButton::Secondary),
        (MouseButton::Middle, PointerButton::Middle),
        (MouseButton::Back, PointerButton::Other(3)),
        (MouseButton::Forward, PointerButton::Other(4)),
        (MouseButton::Other(9), PointerButton::Other(9)),
    ] {
        assert_eq!(pointer_button(button), expected, "{button:?}");
    }
}

#[test]
fn physical_units_are_divided_by_the_scale_and_line_scrolls_are_not() {
    assert_eq!(
        logical_size(PhysicalSize::new(1600, 1200), 2.0),
        Size::new(800.0, 600.0)
    );
    assert_eq!(
        logical_point(PhysicalPosition::new(30.0, 90.0), 1.5),
        Point::new(20.0, 60.0)
    );
    assert_eq!(
        scroll_delta(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(10.0, -40.0)),
            2.0
        ),
        Point::new(5.0, -20.0)
    );
    assert_eq!(
        scroll_delta(MouseScrollDelta::LineDelta(0.0, -2.0), 2.0),
        Point::new(0.0, -80.0)
    );
}
