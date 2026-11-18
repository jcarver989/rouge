#![cfg(target_arch = "wasm32")]

use rouge::core::view;
use rouge::core::{Cursor, InputKind, Key, Point, Rect, Size, Styled, Ui, div, image, text};
use rouge::platform::web::{css_cursor, dom_key};
use rouge::reactive::Runtime;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

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

#[wasm_bindgen_test]
fn bundled_font_shapes_on_the_first_ui_frame() {
    let _entered = Runtime::new().enter();
    let font = include_bytes!("../fixtures/inter-subset.ttf");
    let mut ui = Ui::with_input(
        Size::new(160.0, 40.0),
        InputKind::default(),
        [font.as_slice()],
        || view! { <text font="Inter">"Rouge"</text> },
    );
    assert!(ui.update());
    assert_eq!(ui.text_index_at(Point::new(10.0, 8.0)), Some(1));
}

#[wasm_bindgen_test]
fn byte_backed_images_decode_without_a_native_worker() {
    let _entered = Runtime::new().enter();
    let mut ui = Ui::new(Size::new(160.0, 40.0), || {
        div()
            .flex()
            .items_start()
            .child(image(include_bytes!("../fixtures/red.png").to_vec()).flex_none())
    });
    assert!(ui.update());
    assert_eq!(
        ui.bounds_at(Point::new(1.0, 0.5)),
        Some(Rect::new(0.0, 0.0, 2.0, 1.0))
    );
}
