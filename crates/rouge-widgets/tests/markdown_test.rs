#![cfg(feature = "markdown")]

use rouge_core::testing::{TestResult, TestUi, recorder};
use rouge_core::{Cursor, FrameStats, MemoryClipboard};
use rouge_markdown::{MarkdownDocument, MarkdownStream};
use rouge_reactive::{Runtime, Signal, signal};
use rouge_theme::{MarkdownPalette, Rgba};
use rouge_widgets::markdown;
use rouge_widgets::testing::component_test;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn clicking_explicit_and_bare_links_opens_them() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, actions) = rendered("See [the docs](https://docs.rs) or https://example.com/a.");
    ui.click_text("the docs")?;
    ui.click_text("example.com")?;
    ui.click_text("See")?;
    assert_eq!(
        *actions.borrow(),
        ["https://docs.rs", "https://example.com/a"],
        "clicking plain text opens nothing"
    );
    Ok(())
}

#[test]
fn links_show_a_pointer_only_while_hovered() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, _) = rendered("See https://example.com today");
    let link = ui.text_bounds("example.com").ok_or("link text")?.center();
    let plain = ui.text_bounds("today").ok_or("plain text")?.center();
    ui.pointer_moved(link);
    assert_eq!(ui.cursor(), Cursor::Pointer);
    ui.pointer_moved(plain);
    assert_eq!(ui.cursor(), Cursor::Default);
    Ok(())
}

#[test]
fn the_copy_button_copies_a_code_block() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, _) = rendered("```rust\nfn main() {}\n```\n");
    let clipboard = MemoryClipboard::new();
    ui.set_clipboard(clipboard.clone());
    ui.click_on("markdown-copy-code")?;
    assert_eq!(clipboard.text().as_deref(), Some("fn main() {}\n"));
    Ok(())
}

#[test]
fn compact_code_blocks_are_highlighted_without_the_copy_header() -> TestResult {
    let _entered = Runtime::new().enter();
    let document = MarkdownDocument::parse("Docs\n\n```rust\nfn main() {}\n```\n");
    let ui = component_test()
        .size(600.0, 200.0)
        .build(move || markdown(document).palette(palette()).compact());
    assert!(ui.text_bounds("fn main() {}").is_some(), "the code renders");
    assert_eq!(ui.bounds_of("markdown-copy-code"), None);
    Ok(())
}

#[test]
fn appending_a_block_to_a_stream_builds_only_that_block() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut streamed = Streamed::new("# Title\n\nFirst paragraph.\n\n");
    let appended = streamed.push("Second paragraph.\n\n");
    assert!(streamed.ui.text_bounds("Second paragraph.").is_some());
    assert!(streamed.ui.text_bounds("First paragraph.").is_some());
    assert_eq!(appended.elements_removed, 0, "earlier blocks are kept");
    assert!(appended.elements_created > 0);
    Ok(())
}

#[test]
fn streaming_into_a_code_block_rebuilds_a_fixed_number_of_rows() -> TestResult {
    let _entered = Runtime::new().enter();
    let rebuilt = |lines: usize| {
        let mut streamed = Streamed::new(&format!("```rust\n{}", "let a = 1;\n".repeat(lines)));
        streamed.push("let b = 2;\n").elements_removed
    };
    assert_eq!(rebuilt(3), rebuilt(30), "finished lines are not rebuilt");
    let mut streamed = Streamed::new("```rust\nfn a() {}\n");
    streamed.push("fn b() {}\n```\n");
    let clipboard = MemoryClipboard::new();
    streamed.ui.set_clipboard(clipboard.clone());
    streamed.ui.click_on("markdown-copy-code")?;
    assert_eq!(
        clipboard.text().as_deref(),
        Some("fn a() {}\nfn b() {}\n"),
        "the copy button sees the latest text"
    );
    Ok(())
}

#[test]
fn replacing_a_stream_rebuilds_every_row() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut streamed = Streamed::new("Old text.\n\n");
    let replaced = streamed.change(|stream| stream.replace("New text.\n\n"));
    assert!(streamed.ui.text_bounds("Old text.").is_none());
    assert!(streamed.ui.text_bounds("New text.").is_some());
    assert!(replaced.elements_removed > 0);
    Ok(())
}

fn rendered(source: &str) -> (TestUi, Rc<RefCell<Vec<String>>>) {
    let (links, link) = recorder();
    let document = MarkdownDocument::parse(source);
    let ui = component_test()
        .size(600.0, 200.0)
        .build(move || markdown(document).palette(palette()).on_link(link));
    (ui, links)
}

fn palette() -> MarkdownPalette {
    let color = Rgba::new(80, 120, 200, 255);
    MarkdownPalette {
        heading: color,
        link: color,
        quote: color,
        inline_code: color,
        inline_code_background: color,
        code: color,
        code_background: color,
    }
}

struct Streamed {
    ui: TestUi,
    stream: Signal<MarkdownStream>,
}

impl Streamed {
    fn new(source: &str) -> Self {
        let mut stream = MarkdownStream::new();
        stream.push(source);
        let (ui, stream) = component_test().size(600.0, 800.0).build_with(move || {
            let stream = signal(stream);
            (markdown(stream).palette(palette()), stream)
        });
        Self { ui, stream }
    }

    fn push(&mut self, chunk: &str) -> FrameStats {
        self.change(|stream| stream.push(chunk))
    }

    fn change<T>(&mut self, change: impl FnOnce(&mut MarkdownStream) -> T) -> FrameStats {
        let before = self.ui.stats();
        self.stream.update(|stream| {
            change(stream);
        });
        self.ui.advance(0.0);
        self.ui.stats() - before
    }
}
