use rouge_core::{Clipboard, MemoryClipboard};

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
