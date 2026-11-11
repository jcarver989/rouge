use rouge_core::testing::TestUi;
use rouge_core::{Clipboard, EditAction, MemoryClipboard, editable};
use rouge_reactive::{Runtime, signal};

#[test]
fn a_memory_clipboard_shares_its_text_between_clones() {
    let clipboard = MemoryClipboard::new();
    let mut handle = clipboard.clone();
    assert_eq!(handle.read(), None);
    handle.write("copied");
    assert_eq!(clipboard.text().as_deref(), Some("copied"));
    clipboard.set_text("replaced");
    assert_eq!(handle.read().as_deref(), Some("replaced"));
}

#[test]
fn host_paste_and_cut_share_sanitization_selection_and_undo() -> Result<(), &'static str> {
    let _rt = Runtime::new().enter();
    let value = signal("replace me".to_owned());
    let mut ui = TestUi::new(|| editable(value));
    assert!(ui.clipboard_target().is_none());
    assert!(ui.focus_next(false));
    assert!(ui.dispatch_action(EditAction::SelectAll));
    let target = ui.clipboard_target().ok_or("focused editor")?;
    assert!(ui.paste_text(target, "new\n🦀".into()));
    assert_eq!(value.get(), "new 🦀");
    assert!(ui.dispatch_action(EditAction::Undo));
    assert_eq!(value.get(), "replace me");
    let (target, copied) = ui.clipboard_selection().ok_or("restored selection")?;
    assert_eq!(copied, "replace me");
    assert!(ui.cut_selection(target));
    assert_eq!(value.get(), "");
    assert!(ui.dispatch_action(EditAction::Undo));
    assert_eq!(value.get(), "replace me");
    Ok(())
}

#[test]
fn host_clipboard_rejects_results_after_edit_focus_change_or_remount() -> Result<(), &'static str> {
    let _rt = Runtime::new().enter();
    let value = signal("original".to_owned());
    let mut ui = TestUi::new(|| editable(value));
    assert!(ui.focus_next(false));
    let target = ui.clipboard_target().ok_or("focused editor")?;
    assert!(ui.dispatch_action(EditAction::SelectAll));
    assert!(!ui.paste_text(target, "stale".into()));
    let (target, _) = ui.clipboard_selection().ok_or("selected text")?;
    value.set("new content".into());
    assert!(!ui.cut_selection(target));
    let target = ui.clipboard_target().ok_or("focused editor")?;
    ui.clear_focus();
    assert!(ui.focus_next(false));
    assert!(!ui.paste_text(target, "stale".into()));
    let target = ui.clipboard_target().ok_or("focused editor")?;
    drop(ui);
    let mut ui = TestUi::new(|| editable(value));
    assert!(ui.focus_next(false));
    assert!(!ui.paste_text(target, "stale".into()));
    assert_eq!(value.get(), "new content");
    Ok(())
}
