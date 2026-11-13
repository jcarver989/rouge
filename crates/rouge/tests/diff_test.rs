#[path = "../examples/shared/diff/mod.rs"]
mod diff;
#[allow(dead_code)]
#[path = "../examples/shared/palette.rs"]
mod palette;

use diff::{Patch, SAMPLE, Tone, diff_view};
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
    ui.press("shift-enter");
    ui.type_text("and document the change.");
    assert!(ui.bounds_of("diff-comment-editor").is_some());
    ui.press("enter");
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

#[test]
fn domain_files_render_in_source_order_with_positional_split_pairing() -> TestResult {
    let _rt = Runtime::new().enter();
    let patch = Patch::parse(concat!(
        "diff --git a/before.rs b/z.rs\n",
        "rename from before.rs\n",
        "rename to z.rs\n",
        "--- a/before.rs\n",
        "+++ b/z.rs\n",
        "@@ -1 +1 @@\n",
        "-let old = 1;\n",
        "+let new = 2;\n",
        "diff --git a/a.rs b/a.rs\n",
        "new file mode 100644\n",
        "--- /dev/null\n",
        "+++ b/a.rs\n",
        "@@ -0,0 +1 @@\n",
        "+let café = 1;\n",
        "diff --git a/deleted.rs b/deleted.rs\n",
        "deleted file mode 100644\n",
        "--- a/deleted.rs\n",
        "+++ /dev/null\n",
        "@@ -1 +0,0 @@\n",
        "-old\n",
    ));
    assert_eq!(
        patch
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["z.rs", "a.rs", "deleted.rs"]
    );
    assert_eq!(patch.files[0].renamed_from.as_deref(), Some("before.rs"));
    assert_eq!((patch.files[0].added, patch.files[0].removed), (1, 1));
    let mut ui = TestUiBuilder::default()
        .size(1280.0, 600.0)
        .build(|| diff_view(patch));
    ui.click_on("diff-split")?;
    assert!(ui.bounds_of("diff-line-Split(0, 1)-Old").is_some());
    assert!(ui.bounds_of("diff-line-Split(0, 1)-New").is_some());
    ui.click_on("diff-file-1")?;
    assert!(
        ui.bounds_of("diff-line-Split(1, 1)-New").is_some(),
        "added files retain split mode"
    );
    ui.render()?;
    ui.click_on("diff-file-2")?;
    assert!(
        ui.bounds_of("diff-line-Split(2, 1)-Old").is_some(),
        "deleted files retain split mode"
    );
    Ok(())
}

#[test]
fn compatibility_parser_preserves_plain_paths_truncated_hunks_and_domain_numbering() -> TestResult {
    let source = "--- a/../café.rs\n+++ b/../café.rs\n@@ -8,3 +12,3 @@ context\n same\n-old\n+new\n\\ No newline at end of file\n";
    let patch = Patch::parse(source);
    let file = patch.files.first().ok_or("file")?;
    assert_eq!(file.path, "../café.rs");
    let hunk = file.hunks.first().ok_or("hunk")?;
    assert_eq!(
        (
            hunk.old_start,
            hunk.old_count,
            hunk.new_start,
            hunk.new_count
        ),
        (8, 3, 12, 3)
    );
    assert_eq!(hunk.function_context.as_deref(), Some("context"));
    assert_eq!(
        hunk.lines
            .iter()
            .map(|line| (line.kind, line.old_line_no, line.new_line_no))
            .collect::<Vec<_>>(),
        [
            (Tone::Context, Some(8), Some(12)),
            (Tone::Removed, Some(9), None),
            (Tone::Added, None, Some(13)),
        ]
    );
    assert!(hunk.lines.last().ok_or("last line")?.no_newline);
    Ok(())
}

#[test]
fn canonical_syntax_text_preserves_tabs_unicode_and_trailing_whitespace_in_both_modes() -> TestResult
{
    let _rt = Runtime::new().enter();
    let line = "    let café = 'é';   ";
    let mut ui = review("--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n \tlet café = 'é';   ");
    let mut width = None;
    for (mode, side) in [("Unified", "New"), ("Split", "Old"), ("Split", "New")] {
        if mode == "Split" {
            ui.click_on("diff-split")?;
        }
        let id = format!("diff-line-{mode}(0, 1)-{side}-text");
        let bounds = ui.bounds_of(&id).ok_or("syntax line")?;
        let end = Point::new(bounds.max().x - 1.0, bounds.center().y);
        assert_eq!(
            ui.text_index_at(end),
            Some(line.len() - 1),
            "trailing whitespace remains hit-testable: {mode} {side} {bounds:?} {end:?}"
        );
        if let Some(width) = width {
            assert_eq!(bounds.size.width, width);
        }
        width = Some(bounds.size.width);
    }
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
