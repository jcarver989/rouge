use rouge_core::testing::{BLUE, RED, TestResult, TestUi, TestUiBuilder, WHITE, assert_near};
use rouge_core::{Color, Key, MemoryClipboard, Modifiers, Point, Styled, div, editable};
use rouge_reactive::{Runtime, Signal, signal};

#[test]
fn clipboard_shortcuts_copy_cut_and_paste() {
    let _rt = Runtime::new().enter();
    let value = signal("copy me".to_owned());
    let clipboard = MemoryClipboard::new();
    let mut ui = field(value);
    ui.set_clipboard(clipboard.clone());
    ui.click(Point::new(20.0, 15.0));
    ui.press_with(Key::character("c"), Modifiers::COMMAND);
    assert_eq!(clipboard.text(), None, "nothing is selected yet");
    ui.press_with(Key::character("a"), Modifiers::COMMAND);
    ui.press_with(Key::character("x"), Modifiers::COMMAND);
    assert_eq!(
        (value.get().as_str(), clipboard.text().as_deref()),
        ("", Some("copy me"))
    );
    clipboard.set_text("line one\nline two");
    ui.press_with(Key::character("v"), Modifiers::COMMAND);
    ui.press_with(Key::character("v"), Modifiers::COMMAND);
    assert_eq!(
        value.get(),
        "line one line twoline one line two",
        "newlines are flattened in one line"
    );
}

#[test]
fn caret_and_selection_are_painted_only_while_focused() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal(String::new());
    let mut ui = TestUiBuilder::default().clear(Color::WHITE).build(move || {
        let field = editable(value)
            .w(100.0)
            .text_size(20.0)
            .caret_color(Color::RED)
            .selection_color(Color::BLUE);
        div().size_full().p(10.0).child(field)
    });
    assert_near(ui.pixel(10, 24)?, WHITE, "no caret without focus");
    ui.click(Point::new(20.0, 20.0));
    ui.advance(0.0);
    assert_near(
        ui.pixel(10, 24)?,
        RED,
        "caret at the start of an empty field",
    );
    ui.type_text("MMMM");
    ui.press_with(Key::character("a"), Modifiers::COMMAND);
    ui.advance(0.0);
    let img = ui.render()?;
    assert_near(img.pixel(11, 12), BLUE, "selection behind the text");
    ui.type_text("MMMMMMMMMMMMMMMMMMMM");
    ui.advance(0.0);
    let img = ui.render()?;
    assert_near(
        img.pixel(130, 24),
        WHITE,
        "long text is clipped to the field",
    );
    let caret = ui.text_cursor_area().ok_or("caret")?;
    assert!(
        caret.origin.x <= 110.0,
        "the field scrolls to keep the caret visible: {caret:?}"
    );
    Ok(())
}

fn field(value: Signal<String>) -> TestUi {
    TestUiBuilder::default()
        .build(move || div().size_full().p(10.0).child(editable(value).w_full()))
}
