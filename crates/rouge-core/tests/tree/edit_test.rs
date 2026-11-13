use rouge_core::testing::{TestResult, TestUi, TestUiBuilder};
use rouge_core::{ImeEvent, Key, KeyEvent, Modifiers, Point, PointerButton, Styled, div, editable};
use rouge_reactive::{Runtime, Signal, signal};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn typing_inserts_at_the_caret_and_updates_the_signal() {
    let _rt = Runtime::new().enter();
    let value = signal(String::new());
    let mut ui = field(value);
    ui.type_text("helo");
    assert_eq!(value.get(), "", "keys go nowhere without focus");
    ui.click(Point::new(20.0, 15.0));
    ui.type_text("helo");
    ui.press(Key::ArrowLeft);
    ui.type_text("l");
    assert_eq!(value.get(), "hello");
    ui.press(Key::Backspace);
    ui.press(Key::Delete);
    assert_eq!(value.get(), "hel");
    ui.press(Key::Enter);
    assert_eq!(
        value.get(),
        "hel",
        "a single-line field never takes a newline"
    );
}

#[test]
fn selection_keys_replace_delete_and_move_by_word() {
    let _rt = Runtime::new().enter();
    let value = signal("one two three".to_owned());
    let mut ui = field(value);
    ui.click(Point::new(20.0, 15.0));
    ui.press_with(Key::character("a"), Modifiers::COMMAND);
    ui.type_text("x");
    assert_eq!(value.get(), "x", "typing replaces the selection");

    value.set("one two three".into());
    ui.advance(0.0);
    ui.press(Key::End);
    let word = if cfg!(target_os = "macos") {
        Modifiers::ALT
    } else {
        Modifiers::CTRL
    };
    ui.press_with(Key::Backspace, word);
    assert_eq!(value.get(), "one two ");
    ui.press_with(Key::Home, Modifiers::SHIFT);
    ui.press(Key::Backspace);
    assert_eq!(value.get(), "");
}

#[test]
fn pointer_places_the_caret_selects_words_and_drags() {
    let _rt = Runtime::new().enter();
    let value = signal("alpha beta".to_owned());
    let mut ui = field(value);
    ui.click(Point::new(12.0, 15.0));
    ui.type_text(">");
    assert_eq!(
        value.get(),
        ">alpha beta",
        "a click at the left edge puts the caret first"
    );

    ui.advance(1.0);
    ui.click(Point::new(30.0, 15.0));
    ui.click(Point::new(30.0, 15.0));
    ui.type_text("X");
    assert_eq!(value.get(), ">X beta", "a double click selects the word");

    ui.advance(1.0);
    ui.pointer_moved(Point::new(12.0, 15.0));
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_moved(Point::new(180.0, 15.0));
    ui.pointer_up(PointerButton::Primary);
    ui.type_text("all");
    assert_eq!(value.get(), "all", "dragging selects a range");
}

#[test]
fn ime_composition_is_replaced_until_committed() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal("ab".to_owned());
    let mut ui = field(value);
    ui.click(Point::new(20.0, 15.0));
    assert!(ui.accepts_text());
    ui.press(Key::End);
    ui.ime(ImeEvent::Preedit {
        text: "ni".into(),
        cursor: Some((2, 2)),
    });
    ui.ime(ImeEvent::Preedit {
        text: "nih".into(),
        cursor: Some((3, 3)),
    });
    assert_eq!(value.get(), "abnih");
    ui.ime(ImeEvent::Preedit {
        text: String::new(),
        cursor: None,
    });
    ui.ime(ImeEvent::Commit("日".into()));
    assert_eq!(value.get(), "ab日");
    ui.type_text("!");
    assert_eq!(value.get(), "ab日!");
    let area = ui.text_cursor_area().ok_or("a focused field has a caret")?;
    assert!(
        area.origin.x > 20.0 && area.size.height > 10.0,
        "caret sits after the text: {area:?}"
    );
    Ok(())
}

#[test]
fn multiline_takes_newlines_and_moves_between_lines() {
    let _rt = Runtime::new().enter();
    let value = signal(String::new());
    let submitted = Rc::new(RefCell::new(Vec::new()));
    let sink = submitted.clone();
    let mut ui = TestUiBuilder::default().build(move || {
        div()
            .size_full()
            .child(editable(value).multiline().w_full())
            .child(
                editable(signal("go".to_owned()))
                    .on_submit(move |text| sink.borrow_mut().push(text.to_owned())),
            )
    });
    ui.click(Point::new(20.0, 10.0));
    ui.type_text("ab");
    ui.press(Key::Enter);
    ui.type_text("cd");
    assert_eq!(value.get(), "ab\ncd");
    ui.press(Key::ArrowUp);
    ui.type_text("!");
    assert_eq!(value.get(), "ab!\ncd");
    ui.advance(0.0);
    ui.key_down(KeyEvent::new(Key::Tab));
    ui.press(Key::Enter);
    assert_eq!(
        *submitted.borrow(),
        ["go"],
        "enter submits a single-line field"
    );
}

#[test]
fn composition_enter_does_not_submit_and_host_blur_cancels_preedit() {
    let _rt = Runtime::new().enter();
    let value = signal(String::new());
    let submitted = Rc::new(RefCell::new(Vec::new()));
    let mut ui = TestUi::new(|| {
        let submitted = submitted.clone();
        editable(value).on_submit(move |text| submitted.borrow_mut().push(text.to_owned()))
    });
    assert!(ui.focus_next(false));
    ui.ime(ImeEvent::Preedit {
        text: "に".into(),
        cursor: None,
    });
    ui.press(Key::Enter);
    assert!(submitted.borrow().is_empty());
    ui.ime(ImeEvent::Commit("日本".into()));
    ui.press(Key::Enter);
    assert_eq!(&*submitted.borrow(), &["日本"]);
    ui.ime(ImeEvent::Preedit {
        text: "語".into(),
        cursor: None,
    });
    ui.clear_focus();
    assert_eq!(value.get(), "日本");
    assert!(!ui.accepts_text());
}

#[test]
fn an_empty_preedit_outside_a_composition_leaves_the_selected_text_alone() {
    let _rt = Runtime::new().enter();
    let value = signal("keep me".to_owned());
    let selection = signal(0..0);
    let mut ui = TestUiBuilder::default()
        .build(|| editable(value).selection(selection).w_full().autofocus());
    ui.press_with(Key::character("a"), Modifiers::COMMAND);
    for _ in 0..3 {
        ui.ime(ImeEvent::Preedit {
            text: String::new(),
            cursor: None,
        });
        assert_eq!((value.get().as_str(), selection.get()), ("keep me", 0..7));
    }
}

fn field(value: Signal<String>) -> TestUi {
    TestUiBuilder::default()
        .build(move || div().size_full().p(10.0).child(editable(value).w_full()))
}
