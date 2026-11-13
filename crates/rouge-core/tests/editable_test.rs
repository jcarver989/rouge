#[path = "tree/edit_test.rs"]
mod edit;

use rouge_core::testing::{BLUE, RED, TestResult, TestUi, TestUiBuilder, WHITE, assert_near};
use rouge_core::{
    ClipboardMedia, Color, EditAction, ImeEvent, Key, MemoryClipboard, Point, Styled, div, editable,
};
use rouge_reactive::{Runtime, Signal, signal};
use rouge_render::headless::Image;

#[test]
fn clipboard_shortcuts_copy_cut_and_paste() {
    let _rt = Runtime::new().enter();
    let value = signal("copy me".to_owned());
    let clipboard = MemoryClipboard::new();
    let mut ui = field(value);
    ui.set_clipboard(clipboard.clone());
    ui.click(Point::new(20.0, 15.0));
    ui.press("cmd-c");
    assert_eq!(clipboard.text(), None, "nothing is selected yet");
    ui.press("cmd-a");
    ui.press("cmd-x");
    assert_eq!(
        (value.get().as_str(), clipboard.text().as_deref()),
        ("", Some("copy me"))
    );
    clipboard.set_text("line one\nline two");
    ui.press("cmd-v");
    ui.press("cmd-v");
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
    ui.press("cmd-a");
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
    ui.press("cmd-shift-z");
    assert_eq!(value.get(), "hello");
    selection.set(1..4);
    ui.advance(0.0);
    ui.type_text("i");
    assert_eq!(value.get(), "hio");
    ui.press("cmd-z");
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
    ui.press("shift-enter");
    ui.type_text("b");
    ui.press("shift-enter");
    ui.type_text("c");
    assert_eq!(ui.bounds_of("field").ok_or("field")?.size.height, 60.0);
    ui.press("shift-enter");
    ui.type_text("d");
    ui.press("enter");
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
    ui.press("left");
    assert_near(ui.pixel(0, 10)?, RED, "navigation resets blink");
    ui.advance(0.6);
    assert_near(ui.pixel(0, 10)?, WHITE, "hidden again");
    ui.advance(10.0);
    assert_near(ui.pixel(0, 10)?, RED, "idle caret stays visible");
    assert!(!ui.needs_frame());
    assert_eq!(ui.stats().layouts, layouts);
    Ok(())
}

#[test]
fn a_key_handler_that_stops_a_key_keeps_the_editor_from_handling_it() {
    let _rt = Runtime::new().enter();
    let value = signal("ab".to_owned());
    let intercepted = signal(0);
    let mut ui = TestUiBuilder::default().size(200.0, 40.0).build(move || {
        editable(value)
            .multiline()
            .w(180.0)
            .on_key_down(move |event| {
                if event.key == Key::ArrowUp {
                    intercepted.update(|count| *count += 1);
                    event.stop_propagation();
                }
            })
    });
    ui.click(Point::new(15.0, 10.0));
    assert!(ui.press("up"));
    ui.type_text("c");
    assert_eq!(intercepted.get(), 1);
    assert!(
        value.get().contains('c'),
        "other keys still edit: {}",
        value.get()
    );
}

#[test]
fn pasting_media_goes_to_the_paste_handler_instead_of_the_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal(String::new());
    let pasted = signal(None);
    let clipboard = MemoryClipboard::new();
    let mut ui = TestUi::new(move || {
        let field = editable(value)
            .test_id("field")
            .w(150.0)
            .on_paste(move |media| pasted.set(Some(media)));
        div().size_full().child(field)
    });
    ui.set_clipboard(clipboard.clone());
    ui.click_on("field")?;
    clipboard.set_text("plain");
    ui.press("cmd-v");
    assert_eq!((value.get().as_str(), pasted.get()), ("plain", None));
    let media = ClipboardMedia::Files(vec!["/tmp/a.png".into()]);
    clipboard.set_media(Some("a.png".to_owned()), media.clone());
    ui.press("cmd-v");
    assert_eq!((value.get().as_str(), pasted.get()), ("plain", Some(media)));
    Ok(())
}

#[test]
fn a_placeholder_paints_only_while_the_editor_is_empty() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal(String::new());
    let mut ui = TestUiBuilder::default().clear(Color::WHITE).build(move || {
        let field = editable(value)
            .w(150.0)
            .text_size(20.0)
            .color(Color::BLUE)
            .placeholder("Type here", Color::RED);
        div().size_full().child(field)
    });
    let red = |image: &Image| {
        (0..150).any(|x| (0..30).any(|y| image.pixel(x, y)[0] > 200 && image.pixel(x, y)[2] < 80))
    };
    assert!(red(&ui.render()?), "the placeholder shows while empty");
    value.set("typed".to_owned());
    ui.advance(0.0);
    assert!(!red(&ui.render()?), "typing hides the placeholder");
    Ok(())
}

fn field(value: Signal<String>) -> TestUi {
    TestUiBuilder::default()
        .build(move || div().size_full().p(10.0).child(editable(value).w_full()))
}
