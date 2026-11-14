#![cfg(feature = "markdown")]

use rouge_core::testing::{TestResult, TestUi, recorder};
use rouge_core::{Cursor, FrameStats};
use rouge_markdown::{MarkdownDocument, MarkdownStream};
use rouge_reactive::{Runtime, Signal, memo, signal};
use rouge_syntax::SyntaxTheme;
use rouge_theme::{MarkdownPalette, Rgba};
use rouge_widgets::testing::component_test;
use rouge_widgets::{MarkdownAction, MarkdownPaint, MarkdownRows, markdown};
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
        [open("https://docs.rs"), open("https://example.com/a")],
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
    let (mut ui, actions) = rendered("```rust\nfn main() {}\n```\n");
    ui.click_on("markdown-copy-code")?;
    assert_eq!(
        *actions.borrow(),
        [MarkdownAction::CopyCode("fn main() {}\n".to_owned())]
    );
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
    streamed.ui.click_on("markdown-copy-code")?;
    assert_eq!(
        *streamed.actions.borrow(),
        [MarkdownAction::CopyCode(
            "fn a() {}\nfn b() {}\n".to_owned()
        )],
        "the copy button sees the latest text"
    );
    Ok(())
}

#[test]
fn replacing_a_stream_rebuilds_every_row() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut streamed = Streamed::new("Old text.\n\n");
    streamed.stream.replace("New text.\n\n");
    let replaced = streamed.sync();
    assert!(streamed.ui.text_bounds("Old text.").is_none());
    assert!(streamed.ui.text_bounds("New text.").is_some());
    assert!(replaced.elements_removed > 0);
    Ok(())
}

fn rendered(source: &str) -> (TestUi, Rc<RefCell<Vec<MarkdownAction>>>) {
    let (actions, act) = recorder();
    let rows = MarkdownRows::new(MarkdownDocument::parse(source).blocks());
    let ui = component_test().size(600.0, 200.0).build(move || {
        let paint = MarkdownPaint::new(memo(palette), memo(SyntaxTheme::default), act);
        markdown(signal(rows).read_only(), paint)
    });
    (ui, actions)
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

fn open(url: &str) -> MarkdownAction {
    MarkdownAction::OpenLink(url.to_owned())
}

struct Streamed {
    ui: TestUi,
    stream: MarkdownStream,
    rows: Signal<MarkdownRows>,
    actions: Rc<RefCell<Vec<MarkdownAction>>>,
}

impl Streamed {
    fn new(source: &str) -> Self {
        let (actions, act) = recorder();
        let mut stream = MarkdownStream::new();
        stream.push(source);
        let mut rows = MarkdownRows::default();
        rows.sync(&stream);
        let (ui, rows) = component_test().size(600.0, 800.0).build_with(move || {
            let paint = MarkdownPaint::new(memo(palette), memo(SyntaxTheme::default), act);
            let rows = signal(rows);
            (markdown(rows.read_only(), paint), rows)
        });
        Self {
            ui,
            stream,
            rows,
            actions,
        }
    }

    fn push(&mut self, chunk: &str) -> FrameStats {
        self.stream.push(chunk);
        self.sync()
    }

    fn sync(&mut self) -> FrameStats {
        let before = self.ui.stats();
        let stream = &self.stream;
        self.rows.update(|rows| rows.sync(stream));
        self.ui.advance(0.0);
        self.ui.stats() - before
    }
}
