use rouge_core::testing::{TestResult, TestUi};
use rouge_core::text::{
    Granularity, Movement, SUBPIXEL_BUCKETS, SpanStyle, TextLayout, TextStyle, TextSystem,
    TextWrap, WrapIndent, byte_offset, utf16_offset,
};
use rouge_core::{
    Canvas, Color, Point, Styled, TextAlign, TextRun, canvas, div, editable, rich_text, text,
};
use rouge_reactive::{Runtime, Signal, signal};

#[test]
fn utf16_and_byte_offsets_convert_and_floor_inside_a_character() {
    let text = "a日😀b";
    let pairs = [(0, 0), (1, 1), (4, 2), (8, 4), (9, 5)];
    for (byte, utf16) in pairs {
        assert_eq!(utf16_offset(text, byte), utf16, "byte {byte}");
        assert_eq!(byte_offset(text, utf16), byte, "utf16 {utf16}");
    }
    assert_eq!(
        utf16_offset(text, 6),
        2,
        "inside the emoji floors to its start"
    );
    assert_eq!(
        byte_offset(text, 3),
        4,
        "between surrogates floors to the pair"
    );
    assert_eq!((utf16_offset(text, 99), byte_offset(text, 99)), (5, 9));
}

#[test]
fn layout_measures_scales_and_wraps() {
    let mut text = TextSystem::new();
    let layout = text.layout(
        "The quick brown fox jumps over the lazy dog",
        &style(16.0),
        None,
    );
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

    let layout = text.layout(
        "The quick brown fox jumps over the lazy dog",
        &style(16.0),
        Some(width / 3.0),
    );
    assert!(layout.line_count() >= 3);
    assert!(layout.size().0 <= width / 3.0 + 0.5);
    assert!(layout.size().1 > height * 2.5);

    let (min, max) = layout.content_widths();
    assert!((max - width).abs() < 0.5);
    assert!(min > 10.0 && min < width / 3.0, "longest word, got {min}");

    let large = text.layout(
        "The quick brown fox jumps over the lazy dog",
        &style(32.0),
        None,
    );
    let ratio = large.size().0 / width;
    assert!(
        (1.6..2.2).contains(&ratio),
        "roughly doubles (UI fonts tighten tracking), got {ratio}"
    );

    let unwrapped = TextStyle {
        wrap: TextWrap::NoWrap,
        ..style(16.0)
    };
    let layout = text.layout("several words in a row", &unwrapped, Some(20.0));
    assert_eq!(layout.line_count(), 1, "no-wrap text ignores the width");
}

#[test]
fn glyphs_rasterize_to_coverage_masks() {
    let mut text = TextSystem::new();
    let layout = text.layout("Hi there", &style(16.0), None);
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
    let layout = text.layout("I", &style(16.0), None);
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

#[test]
fn overlapping_spans_compose_per_field_and_later_values_win() {
    let style = TextStyle {
        spans: vec![
            (
                0..6,
                SpanStyle {
                    color: Some(Color::RED),
                    weight: Some(700.0),
                    italic: Some(true),
                    ..Default::default()
                },
            ),
            (
                2..4,
                SpanStyle {
                    background: Some(Color::BLUE),
                    italic: Some(false),
                    ..Default::default()
                },
            ),
            (
                3..5,
                SpanStyle {
                    color: Some(Color::GREEN),
                    ..Default::default()
                },
            ),
        ],
        ..Default::default()
    };
    let layout = TextSystem::new().layout("abcdef", &style, None);
    let mut seen = Vec::new();
    layout.for_each_glyph(|glyph| seen.push(glyph.style.clone()));
    assert_eq!(seen.len(), 6);
    assert_eq!(seen[0].color, Some(Color::RED));
    assert_eq!(seen[2].background, Some(Color::BLUE));
    assert_eq!(seen[2].italic, Some(false));
    assert_eq!(seen[3].weight, Some(700.0));
    assert_eq!(seen[3].color, Some(Color::GREEN));
    assert_eq!(seen[3].background, Some(Color::BLUE));
    assert_eq!(seen[4].background, None);
}

#[test]
fn tabs_render_as_blank_space() {
    let mut text = TextSystem::new();
    for style in [style(16.0), monospace()] {
        let layout = text.layout("\tx", &style, None);
        let mut glyphs = Vec::new();
        let mut images = 0;
        layout.for_each_glyph(|glyph| {
            glyphs.push(glyph.glyph_id);
            let (key, _, _) = glyph.snap((0.0, 0.0), 1.0);
            images += usize::from(text.rasterize(&glyph, &key).is_some());
        });
        assert_eq!(glyphs.len(), 2);
        assert!(
            !glyphs.contains(&0),
            "{} draws no notdef glyph, got {glyphs:?}",
            style.family
        );
        assert_eq!(images, 1, "{} only draws the x", style.family);
    }
}

#[test]
fn tabs_advance_to_the_next_tab_stop() {
    let mut text = TextSystem::new();
    for (tabbed, spaced) in [
        ("\tx", "    x"),
        ("ab\tx", "ab  x"),
        ("abcd\tx", "abcd    x"),
        ("a\t\tx", "a       x"),
        ("abcdef\nab\tx", "abcdef\nab  x"),
    ] {
        let tabbed_x = last_glyph_x(&mut text, tabbed, &monospace());
        let spaced_x = last_glyph_x(&mut text, spaced, &monospace());
        assert!(
            (tabbed_x - spaced_x).abs() < 0.01,
            "{tabbed:?} places x at {tabbed_x}, {spaced:?} at {spaced_x}"
        );
    }
    let two = TextStyle {
        tab_size: 2,
        ..monospace()
    };
    let tabbed_x = last_glyph_x(&mut text, "a\tx", &two);
    let spaced_x = last_glyph_x(&mut text, "a x", &two);
    assert!((tabbed_x - spaced_x).abs() < 0.01);
}

#[test]
fn tabs_keep_byte_offsets_for_hit_testing_and_selection() {
    let mut text = TextSystem::new();
    let tabbed = text.layout("\tx = 1", &monospace(), None);
    let spaced = text.layout("    x = 1", &monospace(), None);
    let (width, height) = spaced.size();
    assert!((tabbed.size().0 - width).abs() < 0.01);

    let column = width / 9.0;
    let y = height / 2.0;
    assert_eq!(tabbed.index_at(column * 4.5, y), Some(1));
    assert_eq!(tabbed.index_at(column * 6.5, y), Some(3));
    assert_eq!(tabbed.index_at(column * 2.0, y), Some(0));

    let caret = tabbed.caret(tabbed.select(1, 1), 1.0);
    assert!((caret.x - spaced.caret(spaced.select(4, 4), 1.0).x).abs() < 0.01);
    let tab = tabbed.selection_rects(tabbed.select(0, 1));
    assert_eq!(tab.len(), 1);
    assert!((tab[0].width - column * 4.0).abs() < 0.01);
    let right = tabbed.move_selection(tabbed.select(0, 0), Movement::Right, false);
    assert_eq!(right.focus(), 1);
    assert_eq!(
        tabbed
            .select_at(column * 6.5, y, Granularity::Caret)
            .focus(),
        3
    );
}

#[test]
fn wrapping_breaks_overlong_tokens_but_prefers_word_boundaries() {
    let mut text = TextSystem::new();
    let column = column_width(&mut text);
    let token = text.layout(
        &"x".repeat(40),
        &code(WrapIndent::None),
        Some(column * 10.5),
    );
    assert_eq!(
        token.line_count(),
        4,
        "an unbreakable token wraps at the edge"
    );
    assert!(token.size().0 <= column * 10.5 + 0.5);

    let words = text.layout(
        "aaaa bbbb cccc",
        &code(WrapIndent::None),
        Some(column * 10.5),
    );
    assert_eq!(words.line_count(), 2);
    let second = words.size().1 * 0.75;
    assert_eq!(
        words.index_at(column * 0.5, second),
        Some(10),
        "the break falls on a space"
    );
}

#[test]
fn continuation_lines_hang_under_the_leading_indent_capped_at_half_the_width() {
    let mut text = TextSystem::new();
    let column = column_width(&mut text);
    let deep = format!("{}deeply(nested, call, with, arguments);", " ".repeat(30));
    let cases = [
        ("    let total = alpha + beta + gamma + delta;", 20.5, 6.0),
        ("\tcall(first, second, third, fourth);", 16.5, 6.0),
        (deep.as_str(), 20.0, 10.0),
    ];
    for (source, columns, hang) in cases {
        let indent = WrapIndent::Leading { extra_columns: 2 };
        let layout = text.layout(source, &code(indent), Some(column * columns));
        let starts = line_starts(&layout);
        assert!(starts.len() >= 2, "{source:?} wraps");
        assert!(
            starts[1..]
                .iter()
                .all(|start| (start - column * hang).abs() < 0.01),
            "{source:?}: {starts:?}"
        );
    }
}

#[test]
fn truncated_text_that_fits_is_laid_out_like_any_other() {
    let mut system = TextSystem::new();
    let natural = system.layout(LABEL, &style(16.0), None).size().0;
    for align in [TextAlign::Start, TextAlign::Center, TextAlign::End] {
        let [fits, wraps] = [TextWrap::Truncate, TextWrap::Wrap].map(|wrap| {
            let style = TextStyle {
                wrap,
                align,
                ..style(16.0)
            };
            glyphs(&system.layout(LABEL, &style, Some(natural + 20.0)))
        });
        assert_eq!(fits, wraps, "{align:?}");
    }
}

#[test]
fn truncated_text_ends_in_an_ellipsis_within_its_width() {
    let mut system = TextSystem::new();
    let natural = system.layout(LABEL, &truncated(), None);
    let full = glyphs(&natural);
    let ellipsis = glyphs(&system.layout("…", &truncated(), None))[0].0;
    let (widest, _) = natural.size();
    let mut shown = 0;
    for width in (20..widest as u32).step_by(7).map(|width| width as f32) {
        let layout = system.layout(LABEL, &truncated(), Some(width));
        let painted = glyphs(&layout);

        assert_eq!(layout.line_count(), 1);
        assert!(layout.size().0 <= width, "{width}: {:?}", layout.size());
        assert_eq!(layout.size().1, natural.size().1, "{width}");
        let (last, kept) = painted.split_last().expect("the ellipsis is painted");
        assert_eq!(last.0, ellipsis, "{width}");
        assert_eq!(kept, &full[..kept.len()], "{width}: keeps its beginning");
        assert!(kept.len() >= shown, "{width}: more room never shows less");
        shown = kept.len();
    }
    assert!(shown > LABEL.len() / 2, "kept {shown} glyphs");
}

#[test]
fn truncated_text_paints_nothing_beyond_a_width_too_small_for_the_ellipsis() {
    let layout = TextSystem::new().layout(LABEL, &truncated(), Some(1.0));
    assert_eq!(glyphs(&layout), []);
}

#[test]
fn truncated_text_gives_way_down_to_its_ellipsis() {
    let mut system = TextSystem::new();
    let ellipsis = system.layout("…", &truncated(), None).size().0;
    let (min, max) = system.layout(LABEL, &truncated(), None).content_widths();
    assert_eq!(min, ellipsis);
    assert_eq!(max, system.layout(LABEL, &style(16.0), None).size().0);

    let (min, max) = system.layout("i", &truncated(), None).content_widths();
    assert_eq!(min, max, "text narrower than an ellipsis is never replaced");
}

#[test]
fn truncated_text_keeps_decorations_and_selections_within_what_it_shows() {
    let mut system = TextSystem::new();
    let style = TextStyle {
        strikethrough: true,
        ..truncated()
    };
    let layout = system.layout(LABEL, &style, Some(80.0));
    layout.for_each_rect(true, |rect, _| {
        assert!(rect.x + rect.width <= 80.0, "{rect:?}");
    });
    let rects = layout.selection_rects(layout.select(0, LABEL.len()));
    assert_ne!(rects, []);
    assert!(
        rects.iter().all(|rect| rect.x + rect.width <= 80.0),
        "{rects:?}"
    );
    assert_eq!(
        layout.select(0, LABEL.len()).range(),
        0..LABEL.len(),
        "selecting it all selects the hidden text too"
    );
}

#[test]
fn a_truncated_label_yields_its_room_to_rigid_siblings_without_help_from_its_ancestors()
-> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div().size_full().flex().items_start().child(
            div()
                .w(160.0)
                .flex()
                .child(
                    div()
                        .flex()
                        .child(div().test_id("label").child(text(LABEL).truncate())),
                )
                .child(div().test_id("rigid").flex_none().size(60.0)),
        )
    });
    let (label, rigid) = (ui.rect_of("label")?, ui.rect_of("rigid")?);
    assert_eq!(label.size.width, 100.0);
    assert_eq!(rigid.origin.x, 100.0);
    assert!(
        ui.text_bounds(LABEL).is_some(),
        "the whole text is still there"
    );
    Ok(())
}

#[test]
fn hits_share_one_shaping_and_span_colors_apply_at_paint() {
    let mut text = TextSystem::new();
    let red = text.layout("abcdef", &tinted(Color::RED), Some(30.0));
    let blue = text.layout("abcdef", &tinted(Color::BLUE), Some(30.0));
    let stats = text.stats();
    assert_eq!((stats.shapes, stats.breaks, stats.cache_hits), (1, 0, 1));
    let colors = |layout: &TextLayout| {
        let mut colors = Vec::new();
        layout.for_each_glyph(|glyph| colors.push(glyph.style.color));
        colors
    };
    assert_eq!(colors(&red)[..3], [Some(Color::RED); 3]);
    assert_eq!(colors(&blue)[..3], [Some(Color::BLUE); 3]);
    assert_eq!(colors(&blue)[3..], [None; 3]);
    let wide = text.layout("abcdef", &tinted(Color::GREEN), Some(300.0));
    assert_eq!((text.stats().shapes, text.stats().breaks), (1, 1));
    assert_eq!(wide.line_count(), 1);
}

#[test]
fn steady_frames_neither_shape_nor_break_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let tint = signal(Color::RED);
    let mut ui = TestUi::new(move || {
        let field = editable(signal(String::new()))
            .test_id("field")
            .w(150.0)
            .placeholder("Type here", Color::RED);
        let body = rich_text("Wrapped body text that spans a few lines", move || {
            vec![(0..7, span(tint.get()))]
        })
        .w(120.0);
        div()
            .size_full()
            .flex()
            .col()
            .child(field)
            .child(body)
            .child(paragraph(tint))
    });
    ui.render()?;
    ui.click_on("field")?;
    ui.run(1.0);
    ui.render()?;
    let before = ui.stats().text;
    for color in [Color::BLUE, Color::GREEN, Color::RED] {
        tint.set(color);
        ui.run(0.6);
        ui.render()?;
    }
    let steady = ui.stats().text - before;
    assert_eq!((steady.shapes, steady.breaks), (0, 0), "{steady:?}");
    assert!(steady.cache_hits > 0);
    Ok(())
}

const LABEL: &str = "Thread trailing commas through the grammar";

#[test]
fn text_that_fits_is_broken_once_whatever_room_it_has() {
    let mut text = TextSystem::new();
    let natural = text.layout("one line", &style(14.0), None).size().0;
    let positions = glyphs(&text.layout("one line", &style(14.0), None));
    for room in [natural.ceil(), natural.ceil() + 1.0, natural * 3.0, 1000.0] {
        let layout = text.layout("one line", &style(14.0), Some(room));
        assert_eq!(layout.line_count(), 1, "{room}");
        assert_eq!(glyphs(&layout), positions, "{room}");
    }
    assert_eq!((text.stats().shapes, text.stats().breaks), (1, 0));
    let narrow = text.layout("one line", &style(14.0), Some(natural * 0.7));
    assert_eq!((narrow.line_count(), text.stats().breaks), (2, 1));
}

#[test]
fn text_placed_by_its_width_is_placed_again_for_each_width() {
    let mut text = TextSystem::new();
    let centered = TextStyle {
        align: TextAlign::Center,
        ..style(14.0)
    };
    let cases = [
        ("centered", &centered),
        ("\u{5e9}\u{5dc}\u{5d5}\u{5dd}", &style(14.0)),
    ];
    for (content, style) in cases {
        let natural = text.layout(content, style, None).size().0.ceil();
        let near = first_glyph_x(&text.layout(content, style, Some(natural)));
        let far = first_glyph_x(&text.layout(content, style, Some(natural + 100.0)));
        assert!(far > near + 40.0, "{content}: {near} then {far}");
    }
}

#[test]
fn only_decorated_text_has_rects() {
    let mut text = TextSystem::new();
    let count = |layout: &TextLayout, foreground| {
        let mut count = 0;
        layout.for_each_rect(foreground, |_, _| count += 1);
        count
    };
    let plain = text.layout("plain", &style(14.0), None);
    assert_eq!((count(&plain, false), count(&plain, true)), (0, 0));
    let struck = TextStyle {
        strikethrough: true,
        ..style(14.0)
    };
    let struck = text.layout("plain", &struck, None);
    assert_eq!((count(&struck, false), count(&struck, true)), (0, 1));
    let spans = vec![
        (
            0..2,
            SpanStyle {
                background: Some(Color::RED),
                ..SpanStyle::default()
            },
        ),
        (
            2..5,
            SpanStyle {
                strikethrough: Some(true),
                ..SpanStyle::default()
            },
        ),
    ];
    let spanned = TextStyle {
        spans,
        ..style(14.0)
    };
    let spanned = text.layout("plain", &spanned, None);
    assert_eq!((count(&spanned, false), count(&spanned, true)), (1, 1));
}

fn truncated() -> TextStyle {
    TextStyle {
        wrap: TextWrap::Truncate,
        ..style(16.0)
    }
}

fn first_glyph_x(layout: &TextLayout) -> f32 {
    let xs = glyphs(layout).into_iter().map(|(_, x)| x);
    xs.fold(f32::INFINITY, f32::min)
}

fn glyphs(layout: &TextLayout) -> Vec<(u32, f32)> {
    let mut glyphs = Vec::new();
    layout.for_each_glyph(|glyph| glyphs.push((glyph.glyph_id, glyph.x)));
    glyphs
}

fn style(size: f32) -> TextStyle {
    TextStyle {
        size,
        ..Default::default()
    }
}

fn shaped(content: &str, max_width: Option<f32>) -> TextLayout {
    TextSystem::new().layout(content, &style(16.0), max_width)
}

fn monospace() -> TextStyle {
    TextStyle {
        family: "monospace".into(),
        size: 16.0,
        wrap: TextWrap::NoWrap,
        ..Default::default()
    }
}

fn last_glyph_x(text: &mut TextSystem, content: &str, style: &TextStyle) -> f32 {
    let layout = text.layout(content, style, None);
    let mut x = 0.0;
    layout.for_each_glyph(|glyph| x = glyph.x);
    x
}

fn code(wrap_indent: WrapIndent) -> TextStyle {
    TextStyle {
        wrap: TextWrap::Wrap,
        wrap_indent,
        ..monospace()
    }
}

fn column_width(text: &mut TextSystem) -> f32 {
    text.layout("0123456789", &monospace(), None).size().0 / 10.0
}

fn line_starts(layout: &TextLayout) -> Vec<f32> {
    let mut starts: Vec<(f32, f32)> = Vec::new();
    layout.for_each_glyph(|glyph| {
        if starts.last().is_none_or(|(y, _)| (glyph.y - y).abs() > 0.5) {
            starts.push((glyph.y, glyph.x));
        }
    });
    starts.into_iter().map(|(_, x)| x).collect()
}

fn tinted(color: Color) -> TextStyle {
    TextStyle {
        spans: vec![(0..3, span(color))],
        ..style(16.0)
    }
}

fn span(color: Color) -> SpanStyle {
    SpanStyle {
        color: Some(color),
        ..SpanStyle::default()
    }
}

fn paragraph(tint: Signal<Color>) -> Canvas {
    let run = move |width: f32| {
        let style = TextStyle {
            spans: vec![(0..8, span(tint.get()))],
            ..TextStyle::default()
        };
        TextRun::new(
            "Measured canvases wrap their runs to the width they are given",
            Point::ZERO,
            &style,
            Color::BLACK,
        )
        .wrap(width)
    };
    canvas(move |painter| painter.run(run(painter.size().width)))
        .w(140.0)
        .measure(move |measure, width| measure.text(&run(width)).max().y)
}
