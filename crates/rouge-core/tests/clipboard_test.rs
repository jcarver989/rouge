use rouge_core::testing::{TestUi, TestUiBuilder};
use rouge_core::{
    Clipboard, ClipboardMedia, EditAction, MemoryClipboard, Styled, div, editable, window_handle,
};
use rouge_reactive::{Runtime, signal};

#[test]
fn a_memory_clipboard_shares_its_text_between_clones() {
    let clipboard = MemoryClipboard::new();
    let mut handle = clipboard.clone();
    assert_eq!(clipboard.text(), None);
    handle.write("copied");
    assert_eq!(clipboard.text().as_deref(), Some("copied"));
    clipboard.set_text("replaced");
    assert_eq!(handle.text().as_deref(), Some("replaced"));
}

#[test]
fn copied_text_reaches_the_clipboard_without_the_host() {
    let _rt = Runtime::new().enter();
    let clipboard = MemoryClipboard::new();
    let (mut ui, window) = TestUiBuilder::default().build_with(|| (div(), window_handle()));
    ui.set_clipboard(clipboard.clone());
    window.copy_text("review");
    assert!(ui.take_window_commands().is_empty());
    assert_eq!(clipboard.text().as_deref(), Some("review"));
}

#[test]
fn a_paste_answered_later_lands_in_the_editor_with_sanitization_and_undo()
-> Result<(), &'static str> {
    let _rt = Runtime::new().enter();
    let value = signal("replace me".to_owned());
    let clipboard = MemoryClipboard::deferred();
    let mut ui = TestUi::new(|| editable(value));
    ui.set_clipboard(clipboard.clone());
    assert!(ui.focus_next(false));
    assert!(ui.dispatch_action(EditAction::SelectAll));
    clipboard.set_text("new\n🦀");
    assert!(ui.dispatch_action(EditAction::Paste));
    assert_eq!(clipboard.pending(), 1);
    assert_eq!(
        value.get(),
        "replace me",
        "nothing changes until it answers"
    );
    clipboard.answer();
    ui.advance(0.0);
    assert_eq!(value.get(), "new 🦀");
    assert!(ui.dispatch_action(EditAction::Undo));
    assert_eq!(value.get(), "replace me");
    Ok(())
}

#[test]
fn a_late_paste_is_dropped_after_an_edit_focus_change_or_remount() {
    let _rt = Runtime::new().enter();
    let value = signal("original".to_owned());
    let clipboard = MemoryClipboard::deferred();
    clipboard.set_text("stale");
    let mut ui = TestUi::new(|| editable(value));
    ui.set_clipboard(clipboard.clone());
    assert!(ui.focus_next(false));

    ui.dispatch_action(EditAction::Paste);
    value.set("new content".into());
    ui.advance(0.0);
    clipboard.answer();
    ui.advance(0.0);
    assert_eq!(value.get(), "new content", "an edit outdates the request");

    ui.dispatch_action(EditAction::Paste);
    ui.clear_focus();
    assert!(ui.focus_next(false));
    clipboard.answer();
    ui.advance(0.0);
    assert_eq!(value.get(), "new content", "so does a focus change");

    ui.dispatch_action(EditAction::Paste);
    drop(ui);
    let mut ui = TestUi::new(|| editable(value));
    ui.set_clipboard(clipboard.clone());
    assert!(ui.focus_next(false));
    clipboard.answer();
    ui.advance(0.0);
    assert_eq!(value.get(), "new content", "and a remount");
}

#[test]
fn only_editors_with_a_paste_handler_ask_for_media() {
    let _rt = Runtime::new().enter();
    let (plain, rich) = (signal(String::new()), signal(String::new()));
    let pasted = signal(None);
    let clipboard = MemoryClipboard::new();
    let media = ClipboardMedia::Png(vec![1, 2, 3]);
    clipboard.set_media(Some("caption".into()), media.clone());
    let mut ui = TestUi::new(move || {
        div()
            .child(editable(plain).test_id("plain").w(100.0))
            .child(
                editable(rich)
                    .test_id("rich")
                    .w(100.0)
                    .on_paste(move |media| pasted.set(Some(media))),
            )
    });
    ui.set_clipboard(clipboard);
    assert!(ui.focus_next(false));
    ui.dispatch_action(EditAction::Paste);
    assert!(ui.focus_next(false));
    ui.dispatch_action(EditAction::Paste);
    ui.advance(0.0);
    assert_eq!(
        (plain.get().as_str(), rich.get().as_str(), pasted.get()),
        ("caption", "", Some(media))
    );
}
