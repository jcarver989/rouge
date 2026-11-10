use rouge_text::{Granularity, Movement, SUBPIXEL_BUCKETS, TextLayout, TextStyle, TextSystem};

#[test]
fn layout_measures_scales_and_wraps() {
    let mut text = TextSystem::new();
    let mut layout = text.layout("The quick brown fox jumps over the lazy dog", &style(16.0));
    layout.break_lines(None);
    let (width, height) = layout.size();
    assert!(
        width > 100.0 && width < 600.0,
        "plausible width, got {width}"
    );
    assert_eq!(layout.line_count(), 1);
    assert!(
        (height - 16.0 * 1.4).abs() < 1.0,
        "one line of 1.4 line height, got {height}"
    );

    layout.break_lines(Some(width / 3.0));
    assert!(layout.line_count() >= 3);
    assert!(layout.size().0 <= width / 3.0 + 0.5);
    assert!(layout.size().1 > height * 2.5);

    let (min, max) = layout.content_widths();
    assert!((max - width).abs() < 0.5);
    assert!(min > 10.0 && min < width / 3.0, "longest word, got {min}");

    let mut large = text.layout("The quick brown fox jumps over the lazy dog", &style(32.0));
    large.break_lines(None);
    let ratio = large.size().0 / width;
    assert!(
        (1.6..2.2).contains(&ratio),
        "roughly doubles (UI fonts tighten tracking), got {ratio}"
    );

    let unwrapped = TextStyle {
        wrap: false,
        ..style(16.0)
    };
    let mut layout = text.layout("several words in a row", &unwrapped);
    layout.break_lines(Some(20.0));
    assert_eq!(layout.line_count(), 1, "no-wrap text ignores the width");
}

#[test]
fn glyphs_rasterize_to_coverage_masks() {
    let mut text = TextSystem::new();
    let mut layout = text.layout("Hi there", &style(16.0));
    layout.break_lines(None);
    let mut glyphs = 0;
    let mut images = 0;
    let mut last_x = -1.0;
    layout.for_each_glyph(|glyph| {
        glyphs += 1;
        assert!(glyph.x > last_x, "glyphs advance left to right");
        last_x = glyph.x;
        let (key, _, y) = glyph.snap((0.0, 0.0), 2.0);
        assert_eq!(
            f32::from_bits(key.size_bits),
            32.0,
            "rasterized at device size"
        );
        assert!(key.subpixel < SUBPIXEL_BUCKETS);
        assert!(y > 0, "baseline is below the top of the line");
        if let Some(image) = text.rasterize(&glyph, &key) {
            images += 1;
            assert_eq!(image.data.len(), (image.width * image.height) as usize);
            assert!(image.data.iter().any(|a| *a > 200), "has solid coverage");
            assert!(image.height <= 40 && image.width <= 40);
        }
    });
    assert_eq!(glyphs, 8);
    assert_eq!(images, 7, "the space has no image");
}

#[test]
fn subpixel_snapping_rolls_over() {
    let mut text = TextSystem::new();
    let mut layout = text.layout("I", &style(16.0));
    layout.break_lines(None);
    layout.for_each_glyph(|glyph| {
        let base = glyph.x;
        let (key, x, _) = glyph.snap((10.0 - base, 0.0), 1.0);
        assert_eq!((x, key.subpixel), (10, 0));
        let (key, x, _) = glyph.snap((10.26 - base, 0.0), 1.0);
        assert_eq!((x, key.subpixel), (10, 1));
        let (key, x, _) = glyph.snap((10.9 - base, 0.0), 1.0);
        assert_eq!((x, key.subpixel), (11, 0));
    });
}

#[test]
fn selection_moves_by_character_word_and_document() {
    let layout = shaped("hello brave world", None);
    let caret = layout.select(0, 0);
    assert!(caret.is_collapsed());
    let right = layout.move_selection(caret, Movement::Right, false);
    assert_eq!(right.range(), 1..1);
    let word = layout.move_selection(caret, Movement::WordRight, true);
    assert_eq!(word.range(), 0..5, "extends over the first word");
    assert_eq!((word.anchor(), word.focus()), (0, 5));
    let end = layout.move_selection(word, Movement::End, false);
    assert_eq!(end.range(), 17..17);
    let back = layout.move_selection(end, Movement::WordLeft, false);
    assert_eq!(back.focus(), 12);
    let all = layout.move_selection(end, Movement::Start, true);
    assert_eq!((all.range(), all.anchor(), all.focus()), (0..17, 17, 0));
    assert_eq!(
        layout.select(3, 99).range(),
        3..17,
        "indices are clamped to the text"
    );
}

#[test]
fn selection_moves_between_wrapped_lines() {
    let single = shaped("one two three four five six", None);
    let layout = shaped("one two three four five six", Some(single.size().0 / 3.0));
    assert!(layout.line_count() >= 3);
    let start = layout.select(0, 0);
    let down = layout.move_selection(start, Movement::Down, false);
    assert!(down.focus() > 0, "moved to the next line");
    let line_end = layout.move_selection(start, Movement::LineEnd, false);
    assert!(line_end.focus() > 0 && line_end.focus() < down.focus() + 1);
    assert_eq!(layout.move_selection(down, Movement::Up, false).focus(), 0);
    assert_eq!(
        layout
            .move_selection(down, Movement::LineStart, false)
            .focus(),
        down.focus()
    );
}

#[test]
fn points_map_to_carets_words_and_geometry() {
    let layout = shaped("hello world", None);
    let (width, height) = layout.size();
    assert_eq!(
        layout
            .select_at(-5.0, height / 2.0, Granularity::Caret)
            .focus(),
        0
    );
    assert_eq!(
        layout
            .select_at(width + 50.0, height / 2.0, Granularity::Caret)
            .focus(),
        11
    );
    let word = layout.select_at(width - 2.0, height / 2.0, Granularity::Word);
    assert_eq!(word.range(), 6..11);
    let line = layout.select_at(width / 2.0, height / 2.0, Granularity::Line);
    assert_eq!(line.range(), 0..11);
    let start = layout.select_at(-5.0, 1.0, Granularity::Caret);
    for shift in [false, true] {
        let extended = layout.extend_selection(start, width + 50.0, 1.0, shift);
        assert_eq!(extended.range(), 0..11);
    }

    let rects = layout.selection_rects(word);
    assert_eq!(rects.len(), 1);
    assert!(rects[0].x > width / 3.0 && (rects[0].x + rects[0].width - width).abs() < 1.0);
    assert!((rects[0].height - height).abs() < 1.0);
    let caret = layout.caret(layout.select(11, 11), 2.0);
    assert!((caret.x - width).abs() < 2.5 && (caret.width - 2.0).abs() < 0.01);
    assert!(
        layout.selection_rects(layout.select(4, 4)).is_empty(),
        "a caret selects nothing"
    );
}

fn style(size: f32) -> TextStyle {
    TextStyle {
        size,
        ..Default::default()
    }
}

fn shaped(content: &str, max_width: Option<f32>) -> TextLayout {
    let mut layout = TextSystem::new().layout(content, &style(16.0));
    layout.break_lines(max_width);
    layout
}
