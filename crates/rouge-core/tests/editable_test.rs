#[path = "tree/edit_test.rs"]
mod edit;

use rouge_core::testing::{BLUE, RED, TestResult, TestUi, TestUiBuilder, WHITE, assert_near};
use rouge_core::{
    Color, EditAction, ImeEvent, Key, MemoryClipboard, Modifiers, Point, Styled, div, editable,
};
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

#[test]
fn history_groups_typing_restores_selection_and_invalidates_redo_on_new_edits() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal(String::new());
    let selection = signal(0..0);
    let mut ui = TestUiBuilder::default().build(|| editable(value).selection(selection).w_full());
    ui.click(Point::new(5.0, 10.0));
    ui.type_text("hello");
    assert_eq!(selection.get(), 5..5);
    assert!(ui.dispatch_action(EditAction::Undo));
    assert_eq!(value.get(), "");
    assert_eq!(selection.get(), 0..0);
    ui.press_with(
        Key::character("z"),
        Modifiers::COMMAND.with(Modifiers::SHIFT),
    );
    assert_eq!(value.get(), "hello");
    selection.set(1..4);
    ui.advance(0.0);
    ui.type_text("i");
    assert_eq!(value.get(), "hio");
    ui.press_with(Key::character("z"), Modifiers::COMMAND);
    assert_eq!(value.get(), "hello");
    assert_eq!(selection.get(), 1..4);
    ui.type_text("ey");
    assert_eq!(value.get(), "heyo");
    ui.dispatch_action(EditAction::Redo);
    assert_eq!(value.get(), "heyo");
    value.set("external".into());
    ui.advance(0.0);
    ui.dispatch_action(EditAction::Undo);
    assert_eq!(value.get(), "external");
    Ok(())
}

#[test]
fn ime_commit_is_one_undo_transaction_and_selection_clamps_to_utf8_boundaries() {
    let _rt = Runtime::new().enter();
    let value = signal("a".to_owned());
    let selection = signal(1..1);
    let mut ui = TestUiBuilder::default()
        .build(|| editable(value).selection(selection).w_full().autofocus());
    ui.ime(ImeEvent::Preedit {
        text: "n".into(),
        cursor: Some((1, 1)),
    });
    ui.ime(ImeEvent::Preedit {
        text: "ni".into(),
        cursor: Some((2, 2)),
    });
    ui.ime(ImeEvent::Commit("日".into()));
    assert_eq!(value.get(), "a日");
    ui.dispatch_action(EditAction::Undo);
    assert_eq!(value.get(), "a");
    ui.dispatch_action(EditAction::Redo);
    assert_eq!(value.get(), "a日");
    selection.set(2..500);
    ui.advance(0.0);
    assert_eq!(selection.get(), 1..4);
}

#[test]
fn multiline_grows_to_row_limit_scrolls_caret_and_supports_submit_with_shift_newline() -> TestResult
{
    let _rt = Runtime::new().enter();
    let value = signal(String::new());
    let submitted = signal(String::new());
    let selection = signal(0..0);
    let mut ui = TestUiBuilder::default().size(300.0, 200.0).build(|| {
        editable(value)
            .multiline()
            .min_rows(2)
            .max_rows(3)
            .submit_on_enter(true)
            .on_submit(move |value| submitted.set(value.to_owned()))
            .selection(selection)
            .text_size(20.0)
            .line_height(1.0)
            .w(200.0)
            .test_id("field")
            .autofocus()
    });
    assert_eq!(ui.bounds_of("field").ok_or("field")?.size.height, 40.0);
    ui.type_text("a");
    ui.press_with(Key::Enter, Modifiers::SHIFT);
    ui.type_text("b");
    ui.press_with(Key::Enter, Modifiers::SHIFT);
    ui.type_text("c");
    assert_eq!(ui.bounds_of("field").ok_or("field")?.size.height, 60.0);
    ui.press_with(Key::Enter, Modifiers::SHIFT);
    ui.type_text("d");
    ui.press(Key::Enter);
    assert_eq!(submitted.get(), "a\nb\nc\nd");
    let bounds = ui.bounds_of("field").ok_or("field")?;
    let caret = ui.text_cursor_area().ok_or("caret")?;
    assert!(
        caret.origin.y >= bounds.origin.y && caret.max().y <= bounds.max().y + 0.5,
        "{caret:?} {bounds:?}"
    );
    selection.set(0..0);
    ui.advance(0.0);
    assert!(ui.text_cursor_area().ok_or("caret")?.origin.y < 1.0);
    Ok(())
}

#[test]
fn caret_blinks_without_layout_resets_on_activity_and_stops_after_idle_timeout() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().clear(Color::WHITE).build(|| {
        editable(signal(String::new()))
            .caret_color(Color::RED)
            .text_size(20.0)
            .w_full()
            .autofocus()
    });
    let layouts = ui.stats().layouts;
    assert_near(ui.pixel(0, 10)?, RED, "visible initially");
    ui.advance(0.6);
    assert_near(ui.pixel(0, 10)?, WHITE, "hidden half cycle");
    ui.press(Key::ArrowLeft);
    assert_near(ui.pixel(0, 10)?, RED, "navigation resets blink");
    ui.advance(0.6);
    assert_near(ui.pixel(0, 10)?, WHITE, "hidden again");
    ui.advance(10.0);
    assert_near(ui.pixel(0, 10)?, RED, "idle caret stays visible");
    assert!(!ui.needs_frame());
    assert_eq!(ui.stats().layouts, layouts);
    Ok(())
}

fn field(value: Signal<String>) -> TestUi {
    TestUiBuilder::default()
        .build(move || div().size_full().p(10.0).child(editable(value).w_full()))
}
