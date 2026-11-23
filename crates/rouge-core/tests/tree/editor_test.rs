use rouge_core::testing::{TestResult, TestUi, TestUiBuilder};
use rouge_core::text::Movement;
use rouge_core::{EditTransaction, ImeEvent, InputHints, Point, Styled, div, editable};
use rouge_reactive::{Runtime, Signal, signal};

#[test]
fn editor_is_only_available_for_a_focused_field() {
    let _rt = Runtime::new().enter();
    let value = signal("ab".to_owned());
    let mut ui = field(value);
    assert!(ui.editor().is_none());
    assert!(!ui.edit_text(EditTransaction::new().replace(0..0, "x")));
    assert_eq!(
        value.get(),
        "ab",
        "an edit without a focused field is dropped"
    );
    ui.click(Point::new(20.0, 15.0));
    assert!(ui.editor().is_some());
}

#[test]
fn queries_report_text_directed_selection_and_geometry() -> TestResult {
    let _rt = Runtime::new().enter();
    let (_, mut ui) = focused("hello world");
    assert!(ui.edit_text(EditTransaction::new().select(8, 2)));
    let editor = ui.editor().ok_or("focused editor")?;
    assert_eq!(editor.text(), "hello world");
    assert_eq!(editor.selection(), (8, 2), "direction is preserved");
    assert_eq!(editor.marked(), None);
    assert!(!editor.multiline());
    assert_eq!(editor.hints(), InputHints::default());
    let (start, end) = (editor.caret(2), editor.caret(8));
    assert!(start.origin.x < end.origin.x, "{start:?} {end:?}");
    let rects = editor.rects(2..8);
    let [selected] = rects.as_slice() else {
        return Err(format!("one line of selection: {rects:?}").into());
    };
    assert!((selected.origin.x - start.origin.x).abs() < 1.0);
    assert!((selected.origin.x + selected.size.width - end.origin.x).abs() < 1.0);
    assert!(editor.bounds().contains(selected.center()));
    assert_eq!(editor.rects(4..4), vec![editor.caret(4)]);
    assert_eq!(editor.index_at(end.center()), 8);
    assert_eq!(editor.index_at(Point::new(-50.0, 15.0)), 0);
    assert_eq!(editor.index_at(Point::new(5000.0, 15.0)), 11);
    Ok(())
}

#[test]
fn composition_is_marked_until_committed_and_undoes_in_one_step() -> TestResult {
    let _rt = Runtime::new().enter();
    let (value, mut ui) = focused("ab");
    ui.press("end");
    ui.edit_text(EditTransaction::new().replace(2..2, "n").mark(2..3));
    ui.edit_text(EditTransaction::new().replace(2..3, "ni").mark(2..4));
    let editor = ui.editor().ok_or("focused editor")?;
    assert_eq!(
        (editor.text(), editor.marked(), editor.selection()),
        ("abni", Some(2..4), (4, 4))
    );
    ui.edit_text(EditTransaction::new().replace(2..4, "日"));
    let editor = ui.editor().ok_or("focused editor")?;
    assert_eq!((editor.text(), editor.marked()), ("ab日", None));
    ui.press("cmd-z");
    assert_eq!(value.get(), "ab", "the whole composition is one undo step");
    Ok(())
}

#[test]
fn abandoning_a_composition_leaves_no_undo_step() -> TestResult {
    let _rt = Runtime::new().enter();
    let (value, mut ui) = focused("ab");
    ui.press("end");
    ui.type_text("c");
    ui.edit_text(EditTransaction::new().replace(3..3, "ni").mark(3..5));
    ui.edit_text(EditTransaction::new().replace(3..5, ""));
    assert_eq!(value.get(), "abc");
    assert_eq!(ui.editor().ok_or("focused editor")?.marked(), None);
    ui.press("cmd-z");
    assert_eq!(value.get(), "ab", "undo skips the abandoned composition");
    Ok(())
}

#[test]
fn replacing_committed_text_is_atomic_and_one_undo_step() -> TestResult {
    let _rt = Runtime::new().enter();
    let (value, mut ui) = focused("teh cat adn");
    ui.press("end");
    let corrected = EditTransaction::new()
        .replace(8..11, "and")
        .replace(0..3, "the")
        .select(11, 11);
    assert!(ui.edit_text(corrected));
    assert_eq!(value.get(), "the cat and");
    assert_eq!(ui.editor().ok_or("focused editor")?.selection(), (11, 11));
    ui.press("cmd-z");
    assert_eq!(value.get(), "teh cat adn");
    Ok(())
}

#[test]
fn later_replacements_address_the_text_left_by_earlier_ones() {
    let _rt = Runtime::new().enter();
    let (value, mut ui) = focused("ab");
    ui.edit_text(
        EditTransaction::new()
            .replace(0..0, "xyz")
            .replace(3..5, "!"),
    );
    assert_eq!(value.get(), "xyz!");
}

#[test]
fn a_whole_state_update_becomes_the_smallest_replacement() {
    let _rt = Runtime::new().enter();
    let (value, mut ui) = focused("teh 日本 cat");
    for after in [
        "the 日本 cat",
        "the 日曜 cat",
        "the 日曜 cat!",
        "the cat!",
        "the cat!",
    ] {
        let before = value.get();
        assert!(ui.edit_text(EditTransaction::between(&before, after)));
        assert_eq!(value.get(), after);
    }
    assert_eq!(
        EditTransaction::between("same", "same"),
        EditTransaction::new(),
        "identical text needs no replacement"
    );
    assert_eq!(
        EditTransaction::between("日本", "日曜"),
        EditTransaction::new().replace(3..6, "曜"),
        "shared bytes inside a character are not split"
    );
}

#[test]
fn consecutive_typing_transactions_share_an_undo_step() {
    let _rt = Runtime::new().enter();
    let (value, mut ui) = focused("");
    for (index, letter) in ["h", "e", "y"].into_iter().enumerate() {
        ui.edit_text(EditTransaction::new().replace(index..index, letter));
    }
    assert_eq!(value.get(), "hey");
    ui.press("cmd-z");
    assert_eq!(value.get(), "");
}

#[test]
fn stale_and_misaligned_ranges_are_clamped_to_character_boundaries() -> TestResult {
    let _rt = Runtime::new().enter();
    let (value, mut ui) = focused("a日😀");
    ui.edit_text(EditTransaction::new().select(2, 6));
    assert_eq!(ui.editor().ok_or("focused editor")?.selection(), (1, 4));
    ui.edit_text(EditTransaction::new().replace(5..400, "!").mark(3..900));
    let editor = ui.editor().ok_or("focused editor")?;
    assert_eq!(editor.text(), "a日!");
    assert_eq!(editor.marked(), Some(1..5));
    let (start, end) = (9, 3);
    ui.edit_text(EditTransaction::new().replace(start..end, "?"));
    assert_eq!(
        value.get(),
        "a日!?",
        "a reversed range inserts at its start"
    );
    Ok(())
}

#[test]
fn single_line_fields_flatten_newlines_and_multiline_fields_keep_them() -> TestResult {
    let _rt = Runtime::new().enter();
    let (single, multi) = (signal(String::new()), signal(String::new()));
    let mut ui = TestUiBuilder::default().build(move || {
        div()
            .size_full()
            .child(editable(single).w_full().test_id("single"))
            .child(editable(multi).multiline().w_full().test_id("multi"))
    });
    ui.click_on("single")?;
    ui.edit_text(EditTransaction::new().replace(0..0, "a\nb"));
    ui.click_on("multi")?;
    assert!(ui.editor().ok_or("focused editor")?.multiline());
    ui.edit_text(EditTransaction::new().replace(0..0, "a\nb"));
    assert_eq!(
        (single.get().as_str(), multi.get().as_str()),
        ("a b", "a\nb")
    );
    let editor = ui.editor().ok_or("focused editor")?;
    assert!(
        editor.caret(3).origin.y > editor.caret(0).origin.y,
        "the second line sits below the first"
    );
    Ok(())
}

#[test]
fn replacing_the_value_from_outside_drops_the_marked_range() -> TestResult {
    let _rt = Runtime::new().enter();
    let (value, mut ui) = focused("ab");
    ui.edit_text(EditTransaction::new().replace(2..2, "ni").mark(2..4));
    value.set("reset".to_owned());
    ui.advance(0.0);
    let editor = ui.editor().ok_or("focused editor")?;
    assert_eq!((editor.text(), editor.marked()), ("reset", None));
    Ok(())
}

#[test]
fn fields_with_identical_text_have_distinct_stable_identities() -> TestResult {
    let _rt = Runtime::new().enter();
    let (first, second) = (signal("same".to_owned()), signal("same".to_owned()));
    let mut ui = TestUiBuilder::default().build(move || {
        div()
            .size_full()
            .child(editable(first).w_full().test_id("first"))
            .child(editable(second).w_full().test_id("second"))
    });
    ui.click_on("first")?;
    let id = ui.editor().ok_or("focused editor")?.id();
    ui.edit_text(EditTransaction::new().replace(4..4, "!"));
    assert_eq!(ui.editor().ok_or("focused editor")?.id(), id);
    ui.click_on("second")?;
    let editor = ui.editor().ok_or("focused editor")?;
    assert_eq!(editor.text(), "same");
    assert_ne!(
        editor.id(),
        id,
        "an edit meant for the first field can be told apart"
    );
    Ok(())
}

#[test]
fn slices_clamp_and_movement_steps_over_whole_characters() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal("a😀b\nsecond".to_owned());
    let mut ui = TestUiBuilder::default().build(move || {
        div()
            .size_full()
            .child(editable(value).multiline().w_full())
    });
    ui.click(Point::new(20.0, 10.0));
    let editor = ui.editor().ok_or("focused editor")?;
    assert_eq!(editor.slice(0..5), "a😀");
    assert_eq!(
        editor.slice(3..400),
        "😀b\nsecond",
        "a split emoji is kept whole"
    );
    let (start, end) = (6, 2);
    assert_eq!(editor.slice(start..end), "", "a reversed range is empty");
    assert_eq!(editor.moved(1, Movement::Right), 5);
    assert_eq!(editor.moved(5, Movement::Left), 1);
    assert_eq!(
        editor.moved(3, Movement::Right),
        5,
        "a misaligned start is floored"
    );
    assert!(
        editor.moved(0, Movement::Down) > 6,
        "down lands on the second line"
    );
    assert_eq!(editor.moved(8, Movement::Up), 1);
    Ok(())
}

#[test]
fn an_ime_preedit_and_a_marked_transaction_leave_the_same_state() -> TestResult {
    let _rt = Runtime::new().enter();
    let (_, mut by_event) = focused("ab");
    let (_, mut by_transaction) = focused("ab");
    for ui in [&mut by_event, &mut by_transaction] {
        ui.press("end");
    }
    by_event.ime(ImeEvent::Preedit {
        text: "n\ni".to_owned(),
        cursor: Some((1, 1)),
    });
    by_transaction.edit_text(
        EditTransaction::new()
            .replace(2..2, "n\ni")
            .mark(2..5)
            .select(3, 3),
    );
    for ui in [&by_event, &by_transaction] {
        let editor = ui.editor().ok_or("focused editor")?;
        assert_eq!(
            (editor.text(), editor.marked(), editor.selection()),
            ("abn i", Some(2..5), (3, 3)),
            "a single-line preedit is flattened like committed text"
        );
    }
    by_event.ime(ImeEvent::Commit("日".to_owned()));
    by_transaction.edit_text(EditTransaction::new().replace(2..5, "日"));
    for ui in [&mut by_event, &mut by_transaction] {
        let editor = ui.editor().ok_or("focused editor")?;
        assert_eq!((editor.text(), editor.marked()), ("ab日", None));
        ui.press("cmd-z");
        assert_eq!(ui.editor().ok_or("focused editor")?.text(), "ab");
    }
    Ok(())
}

fn focused(text: &str) -> (Signal<String>, TestUi) {
    let value = signal(text.to_owned());
    let mut ui = field(value);
    ui.click(Point::new(20.0, 15.0));
    (value, ui)
}

fn field(value: Signal<String>) -> TestUi {
    TestUiBuilder::default()
        .build(move || div().size_full().p(10.0).child(editable(value).w_full()))
}
