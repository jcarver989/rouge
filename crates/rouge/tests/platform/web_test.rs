#![cfg(target_arch = "wasm32")]

use rouge::core::{Cursor, Key};
use rouge::platform::web::{css_cursor, dom_key};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn named_dom_keys_map_to_rouge_keys() {
    for (key, expected) in [
        ("Enter", Key::Enter),
        ("Escape", Key::Escape),
        ("Tab", Key::Tab),
        (" ", Key::Space),
        ("Backspace", Key::Backspace),
        ("ArrowLeft", Key::ArrowLeft),
        ("PageDown", Key::PageDown),
        ("F7", Key::F(7)),
        ("CapsLock", Key::Other),
    ] {
        assert_eq!(dom_key(key, "", false), expected, "{key}");
    }
}

#[wasm_bindgen_test]
fn shifted_characters_map_to_their_unshifted_key_like_native_hosts() {
    assert_eq!(dom_key("A", "KeyA", true), Key::character("a"));
    assert_eq!(dom_key("?", "Slash", true), Key::character("/"));
    assert_eq!(dom_key("!", "Digit1", true), Key::character("1"));
    assert_eq!(dom_key("é", "Digit2", false), Key::character("é"));
    assert_eq!(dom_key("Ä", "Quote", true), Key::character("'"));
}

#[wasm_bindgen_test]
fn cursors_map_to_css_cursor_names() {
    assert_eq!(css_cursor(Cursor::Text), "text");
    assert_eq!(css_cursor(Cursor::ResizeHorizontal), "ew-resize");
}
