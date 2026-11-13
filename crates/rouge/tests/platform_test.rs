use rouge::core::{Cursor, Key, Modifiers, Point, PointerButton, Size};
use rouge::platform::{
    Wheel, WheelGesture, cursor_icon, logical_point, logical_size, modifiers, named_key,
    pointer_button, scroll_delta,
};
use std::time::{Duration, Instant};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{MouseButton, MouseScrollDelta, TouchPhase};
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

#[test]
fn a_trackpad_flick_drags_releases_and_leaves_the_os_momentum_to_the_compositor() {
    let mut wheel = Wheel::default();
    let start = Instant::now();
    let at = |ms| start + Duration::from_millis(ms);
    let gestures = [
        (TouchPhase::Started, 0.0, 0),
        (TouchPhase::Moved, -20.0, 16),
        (TouchPhase::Ended, 0.0, 33),
        (TouchPhase::Started, -18.0, 40),
        (TouchPhase::Moved, -12.0, 56),
        (TouchPhase::Ended, 0.0, 900),
    ]
    .map(|(phase, y, ms)| wheel.input(pixels(y), phase, 2.0, at(ms)));
    assert_eq!(
        gestures,
        [
            WheelGesture::Drag(Point::ZERO),
            WheelGesture::Drag(Point::new(0.0, -10.0)),
            WheelGesture::Release(Point::ZERO),
            WheelGesture::Coast,
            WheelGesture::Coast,
            WheelGesture::Coast,
        ]
    );
    assert_eq!(
        wheel.input(pixels(-8.0), TouchPhase::Moved, 2.0, at(1000)),
        WheelGesture::Scroll(Point::new(0.0, -4.0)),
        "phaseless pixel deltas scroll"
    );
}

#[test]
fn touching_again_after_the_momentum_gap_or_while_coasting_starts_a_new_drag() {
    let mut wheel = Wheel::default();
    let start = Instant::now();
    let at = |ms| start + Duration::from_millis(ms);
    wheel.input(pixels(0.0), TouchPhase::Started, 1.0, at(0));
    wheel.input(pixels(0.0), TouchPhase::Ended, 1.0, at(20));
    assert_eq!(
        wheel.input(pixels(-5.0), TouchPhase::Started, 1.0, at(200)),
        WheelGesture::Drag(Point::new(0.0, -5.0)),
        "a lift then a later touch is two drags"
    );
    wheel.input(pixels(0.0), TouchPhase::Ended, 1.0, at(220));
    assert_eq!(
        wheel.input(pixels(-9.0), TouchPhase::Started, 1.0, at(225)),
        WheelGesture::Coast
    );
    assert_eq!(
        wheel.input(pixels(-3.0), TouchPhase::Started, 1.0, at(400)),
        WheelGesture::Drag(Point::new(0.0, -3.0)),
        "touching during momentum catches it"
    );
}

#[test]
fn line_deltas_always_scroll_even_mid_gesture() {
    let mut wheel = Wheel::default();
    let now = Instant::now();
    wheel.input(pixels(0.0), TouchPhase::Started, 1.0, now);
    assert_eq!(
        wheel.input(
            MouseScrollDelta::LineDelta(0.0, -1.0),
            TouchPhase::Moved,
            1.0,
            now
        ),
        WheelGesture::Scroll(Point::new(0.0, -40.0))
    );
    assert_eq!(
        wheel.input(pixels(-6.0), TouchPhase::Moved, 1.0, now),
        WheelGesture::Drag(Point::new(0.0, -6.0)),
        "and leave the trackpad gesture alone"
    );
}

fn pixels(y: f64) -> MouseScrollDelta {
    MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, y))
}
