#![cfg(feature = "markdown")]

use rouge_core::Cursor;
use rouge_core::testing::{TestResult, TestUi, recorder};
use rouge_markdown::MarkdownDocument;
use rouge_reactive::{Runtime, memo};
use rouge_syntax::SyntaxTheme;
use rouge_theme::{MarkdownPalette, Rgba};
use rouge_widgets::testing::component_test;
use rouge_widgets::{MarkdownAction, MarkdownPaint, markdown};
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

fn rendered(source: &str) -> (TestUi, Rc<RefCell<Vec<MarkdownAction>>>) {
    let (actions, act) = recorder();
    let document = MarkdownDocument::parse(source);
    let ui = component_test().size(600.0, 200.0).build(move || {
        let paint = MarkdownPaint::new(memo(palette), memo(SyntaxTheme::default), act);
        markdown(document.blocks(), paint)
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
