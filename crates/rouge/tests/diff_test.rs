#[path = "../examples/shared/diff/mod.rs"]
mod diff;
#[allow(dead_code)]
#[path = "../examples/shared/palette.rs"]
mod palette;

use diff::{Patch, SAMPLE, diff_view};
use rouge::core::MemoryClipboard;
use rouge::core::testing::{TestResult, TestUi, TestUiBuilder};
use rouge::prelude::*;
use std::fmt::{Error as FormatError, Write};

#[test]
fn file_navigation_and_mode_switching_use_live_list_geometry() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = review(&patch()?);
    assert!(ui.bounds_of("diff-sticky-file-0").is_some());
    ui.click_on("diff-file-1")?;
    assert!(ui.bounds_of("diff-sticky-file-1").is_some());
    let line = ui.center_of("diff-line-Unified(1, 1)-New")?;
    assert_eq!(line.y, 133.0);

    ui.click_on("diff-split")?;
    assert!(ui.bounds_of("diff-sticky-file-1").is_some());
    let old = ui.center_of("diff-line-Split(1, 1)-Old")?;
    let new = ui.center_of("diff-line-Split(1, 1)-New")?;
    assert_eq!(old.y, line.y);
    assert_eq!(new.y, old.y);
    assert!(new.x > old.x);

    ui.hover_on("diff-scroll")?;
    ui.scroll(Point::new(0.0, -200.0));
    ui.advance(0.0);
    ui.click_on("diff-unified")?;
    assert!(ui.bounds_of("diff-sticky-file-1").is_some());
    let row = ui.center_of("diff-line-Unified(1, 20)-New")?;
    let expected = 159.0;
    assert!((row.y - expected).abs() <= 1.0, "{row:?} versus {expected}");
    ui.click_on("diff-file-0")?;
    assert!(ui.bounds_of("diff-sticky-file-0").is_some());
    assert_eq!(ui.center_of("diff-line-Unified(0, 1)-New")?.y, line.y);
    Ok(())
}

#[test]
fn sticky_header_is_pushed_by_the_next_file_and_collapse_keeps_navigation_live() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = review(&patch()?);
    ui.hover_on("diff-scroll")?;
    ui.scroll(Point::new(0.0, -1352.0));
    ui.advance(0.0);
    assert!(ui.bounds_of("diff-sticky-file-0").is_some());
    assert_eq!(
        ui.bounds_of("diff-sticky")
            .ok_or("missing sticky header")?
            .origin
            .y,
        50.0
    );
    ui.scroll(Point::new(0.0, -30.0));
    ui.advance(0.0);
    assert!(ui.bounds_of("diff-sticky-file-1").is_some());
    assert_eq!(
        ui.bounds_of("diff-sticky")
            .ok_or("missing sticky header")?
            .origin
            .y,
        60.0
    );

    ui.click_on("diff-sticky-file-1")?;
    assert!(ui.bounds_of("diff-line-Unified(1, 1)-New").is_none());
    ui.click_on("diff-file-2")?;
    assert!(ui.bounds_of("diff-sticky-file-2").is_some());
    ui.click_on("diff-file-1")?;
    ui.click_on("diff-sticky-file-1")?;
    assert_eq!(ui.center_of("diff-line-Unified(1, 1)-New")?.y, 133.0);
    Ok(())
}

#[test]
fn group_hover_reveals_comment_control_and_editor_submits_a_local_comment() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = review(&patch()?);
    let clipboard = MemoryClipboard::new();
    ui.set_clipboard(clipboard.clone());
    let button = ui
        .bounds_of("diff-line-Unified(0, 1)-New-comment")
        .ok_or("missing comment control")?;
    let x = (button.origin.x + 3.0) as u32;
    let y = (button.origin.y + 3.0) as u32;
    let idle = ui.pixel(x, y)?;
    ui.hover_on("diff-line-Unified(0, 1)-New")?;
    let hovered = ui.pixel(x, y)?;
    assert_ne!(hovered, idle);
    ui.pointer_left();
    ui.advance(0.0);
    assert_eq!(ui.pixel(x, y)?, idle);

    ui.click_on("diff-line-Unified(0, 1)-New-comment")?;
    assert!(ui.bounds_of("diff-comment-editor").is_some());
    ui.type_text("Keep the Unicode café label");
    ui.press_with(Key::Enter, Modifiers::SHIFT);
    ui.type_text("and document the change.");
    assert!(ui.bounds_of("diff-comment-editor").is_some());
    ui.press(Key::Enter);
    assert!(ui.bounds_of("diff-comment-editor").is_none());
    assert!(ui.bounds_of("diff-comments-saved").is_some());
    ui.click_on("diff-line-Unified(0, 1)-New-comment")?;
    ui.dispatch_action(EditAction::SelectAll);
    ui.dispatch_action(EditAction::Copy);
    assert_eq!(
        clipboard.text().as_deref(),
        Some("Keep the Unicode café label\nand document the change.")
    );
    ui.click_on("diff-comment-cancel")?;
    assert!(ui.bounds_of("diff-comment-editor").is_none());
    Ok(())
}

#[test]
fn sample_rich_text_renders_and_empty_patch_has_no_list() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = review(SAMPLE);
    let image = ui.render()?;
    assert_eq!((image.width, image.height), (1280, 600));
    assert_ne!(image.pixel(262, 160), [0, 0, 0, 0]);
    ui.click_on("diff-split")?;
    ui.render()?;
    let mut empty = review("");
    assert!(empty.bounds_of("diff-scroll").is_none());
    empty.render()?;
    Ok(())
}

#[test]
fn rich_text_byte_hits_and_inherited_rem_typography_share_one_line() -> TestResult {
    let _rt = Runtime::new().enter();
    let source = patch()?;
    let (mut ui, size) = TestUiBuilder::default()
        .size(1280.0, 600.0)
        .build_with(|| (diff_view(Patch::parse(&source)), root_font_size()));
    let id = "diff-line-Unified(0, 1)-New-text";
    let before = ui.bounds_of(id).ok_or("missing syntax line")?;
    let end = Point::new(before.origin.x + before.size.width - 1.0, before.center().y);
    assert_eq!(
        ui.text_index_at(end),
        "let old_0 = \"café\";".rfind(';'),
        "{before:?} {end:?}"
    );
    size.set(20.0);
    ui.advance(0.0);
    let after = ui.bounds_of(id).ok_or("missing resized syntax line")?;
    assert!(after.size.width > before.size.width * 1.2);
    assert!(after.size.height > before.size.height);
    ui.render()?;
    Ok(())
}

fn review(source: &str) -> TestUi {
    TestUiBuilder::default()
        .size(1280.0, 600.0)
        .build(|| diff_view(Patch::parse(source)))
}

fn patch() -> Result<String, FormatError> {
    let mut patch = String::new();
    for file in 0..3 {
        writeln!(
            patch,
            "diff --git a/src/file{file}.rs b/src/file{file}.rs\n--- a/src/file{file}.rs\n+++ b/src/file{file}.rs\n@@ -1,30 +1,30 @@"
        )?;
        for line in 0..30 {
            writeln!(patch, "-let old_{line} = \"café\";")?;
        }
        for line in 0..30 {
            writeln!(patch, "+let new_{line} = \"naïve\";")?;
        }
    }
    Ok(patch)
}
