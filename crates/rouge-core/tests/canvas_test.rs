use rouge_core::testing::{BLUE, GREEN, RED, TestResult, TestUiBuilder, WHITE, assert_near};
use rouge_core::{
    Canvas, Color, Path, Point, Rect, Styled, TextHit, TextRun, TextStyle, canvas, div, icon,
};
use rouge_reactive::{Runtime, Signal, signal};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn canvas_draws_rects_and_filled_and_stroked_paths_in_local_space() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().clear(Color::WHITE).build(|| {
        let drawing = canvas(|painter| {
            let size = painter.size();
            painter.fill_rect(Rect::new(0.0, 0.0, 20.0, size.height), Color::RED);
            let circle = Path::builder().circle(Point::new(50.0, 30.0), 20.0).build();
            painter.fill(&circle, Color::BLUE);
            let line = Path::builder()
                .move_to(80.0, 30.0)
                .line_to(120.0, 30.0)
                .build();
            painter.stroke(&line, 6.0, Color::GREEN);
        });
        div().size_full().p(10.0).child(drawing.w(150.0).h(60.0))
    });
    ui.advance(0.0);
    let img = ui.render()?;
    assert_near(img.pixel(15, 60), RED, "rect spans the canvas height");
    assert_near(img.pixel(60, 40), BLUE, "circle center");
    assert_near(
        img.pixel(42, 22),
        WHITE,
        "outside the circle but inside its box",
    );
    assert_near(img.pixel(110, 40), GREEN, "stroke centered on the line");
    assert_near(img.pixel(110, 45), WHITE, "stroke is 6 wide");
    let partial = (60..82)
        .filter(|x| (20..235).contains(&img.pixel(*x, 26)[0]))
        .count();
    assert!(
        (1..=3).contains(&partial),
        "the circle edge is antialiased over a pixel or two, got {partial}"
    );
    Ok(())
}

#[test]
fn canvas_redraws_when_a_signal_it_reads_changes_without_layout() -> TestResult {
    let _rt = Runtime::new().enter();
    let color = signal(Color::RED);
    let mut ui = TestUiBuilder::default().build(move || {
        canvas(move |painter| {
            let bounds = Rect {
                origin: Point::ZERO,
                size: painter.size(),
            };
            painter.fill(&Path::builder().rect(bounds).build(), color.get());
        })
        .size(40.0)
    });
    ui.advance(0.0);
    assert_near(ui.pixel(20, 20)?, RED, "initial");
    let (layouts, _) = ui.passes();
    color.set(Color::BLUE);
    ui.advance(0.0);
    assert_near(ui.pixel(20, 20)?, BLUE, "repainted");
    assert_eq!(ui.passes().0, layouts, "a canvas redraw needs no layout");
    Ok(())
}

#[test]
fn canvas_text_and_icons_scale_into_their_box() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().clear(Color::WHITE).build(|| {
        let label = canvas(|painter| {
            let style = TextStyle {
                size: 40.0,
                ..TextStyle::default()
            };
            painter.text("MM", Point::new(0.0, 0.0), &style, Color::RED);
        });
        let square = icon(Path::svg("M0 0 H12 V24 H0 Z"))
            .color(Color::BLUE)
            .size(48.0);
        div()
            .flex()
            .size_full()
            .child(label.w(100.0).h(60.0))
            .child(square)
    });
    ui.advance(0.0);
    let img = ui.render()?;
    let reds = (0..100).filter(|x| img.pixel(*x, 30)[1] < 100).count();
    assert!(
        reds > 10,
        "glyphs were drawn by the canvas, got {reds} red pixels"
    );
    assert_near(
        img.pixel(110, 24),
        BLUE,
        "the left half of the view box is filled",
    );
    assert_near(img.pixel(140, 24), WHITE, "the right half is empty");
    Ok(())
}

#[test]
fn named_regions_and_runs_are_found_by_name_and_follow_the_canvas() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().size(300.0, 100.0).build(|| {
        let drawing = canvas(|painter| {
            painter.region("left-half", Rect::new(0.0, 0.0, 75.0, 40.0));
            let style = TextStyle::default();
            painter.run(
                TextRun::new(
                    "wide text that overflows",
                    Point::new(80.0, 0.0),
                    &style,
                    Color::BLACK,
                )
                .clip(Rect::new(80.0, 0.0, 20.0, 40.0))
                .name("label"),
            );
        });
        div().p(10.0).child(drawing.w(150.0).h(40.0))
    });
    ui.advance(0.0);
    let region = ui.bounds_of("left-half").ok_or("region")?;
    assert_eq!(region, Rect::new(10.0, 10.0, 75.0, 40.0));
    let label = ui.bounds_of("label").ok_or("named run")?;
    assert_eq!(label.origin, Point::new(90.0, 10.0));
    assert!(
        label.size.width > 20.0,
        "named runs report their full text extent"
    );
    Ok(())
}

#[test]
fn tagged_runs_report_the_hovered_and_clicked_byte_and_clipped_runs_report_overflow() -> TestResult
{
    let _rt = Runtime::new().enter();
    let hovered = Rc::new(RefCell::new(Vec::new()));
    let clicked = Rc::new(RefCell::new(Vec::new()));
    let overflow = signal(0.0_f32);
    let (seen, pressed) = (hovered.clone(), clicked.clone());
    let mut ui = TestUiBuilder::default().size(300.0, 100.0).build(move || {
        canvas(|painter| {
            let style = TextStyle::default();
            painter.run(
                TextRun::new("hello world", Point::new(0.0, 0.0), &style, Color::BLACK)
                    .clip(Rect::new(0.0, 0.0, 30.0, 40.0))
                    .tag(7),
            );
        })
        .track_overflow(overflow)
        .on_text_hover(move |hit, _| seen.borrow_mut().push(hit))
        .on_text_click(move |hit, _| pressed.borrow_mut().push(hit))
        .w(300.0)
        .h(40.0)
    });
    ui.advance(0.0);
    assert!(
        overflow.get() > 10.0,
        "clipped text overflows by {}",
        overflow.get()
    );
    ui.pointer_moved(Point::new(2.0, 8.0));
    ui.click(Point::new(2.0, 8.0));
    assert_eq!(
        hovered.borrow().last(),
        Some(&Some(TextHit { tag: 7, index: 0 }))
    );
    assert_eq!(
        clicked.borrow().last(),
        Some(&Some(TextHit { tag: 7, index: 0 }))
    );
    ui.pointer_moved(Point::new(200.0, 8.0));
    assert_eq!(
        hovered.borrow().last(),
        Some(&None),
        "text outside its clip is not hit"
    );
    Ok(())
}

#[test]
fn right_aligned_runs_end_at_their_origin() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().size(300.0, 100.0).build(|| {
        canvas(|painter| {
            let style = TextStyle::default();
            painter.run(
                TextRun::new("42", Point::new(100.0, 0.0), &style, Color::BLACK)
                    .right_aligned()
                    .name("number"),
            );
        })
        .w(300.0)
        .h(40.0)
    });
    ui.advance(0.0);
    let number = ui.bounds_of("number").ok_or("number")?;
    assert!((number.max().x - 100.0).abs() < 0.01, "{number:?}");
    assert!(number.origin.x < 100.0);
    Ok(())
}

#[test]
fn measured_canvas_sizes_to_its_wrapped_text_and_rewraps_on_resize() -> TestResult {
    let _rt = Runtime::new().enter();
    let width = signal(400.0_f32);
    let content = signal(PARAGRAPH.to_owned());
    let clicked = Rc::new(RefCell::new(Vec::new()));
    let pressed = clicked.clone();
    let mut ui = TestUiBuilder::default().size(500.0, 600.0).build(move || {
        div().flex().col().w(move || width.get()).child(
            wrapped(content, signal(Color::BLACK))
                .on_text_click(move |hit, _| pressed.borrow_mut().push(hit))
                .element(|canvas| canvas.test_id("wrapped"))
                .w_full(),
        )
    });
    ui.advance(0.0);
    let wide = ui.bounds_of("wrapped").ok_or("canvas")?;
    let body = ui.bounds_of("body").ok_or("named run")?;
    assert!(body.size.height > LINE * 1.5, "the text wraps: {body:?}");
    assert!(
        (wide.size.height - body.size.height - PADDING).abs() <= 1.0,
        "{wide:?} {body:?}"
    );

    width.set(200.0);
    ui.advance(0.0);
    let narrow = ui.bounds_of("wrapped").ok_or("canvas")?;
    let body = ui.bounds_of("body").ok_or("named run")?;
    assert!(
        narrow.size.height > wide.size.height,
        "{wide:?} -> {narrow:?}"
    );
    assert!(
        (narrow.size.height - body.size.height - PADDING).abs() <= 1.0,
        "{narrow:?} {body:?}"
    );
    assert!(body.size.width <= 200.0);

    ui.click(Point::new(2.0, body.max().y - LINE / 2.0));
    let hit = clicked
        .borrow()
        .last()
        .copied()
        .flatten()
        .ok_or("no hit on the last line")?;
    assert!(hit.index > PARAGRAPH.len() / 2, "{hit:?}");
    Ok(())
}

#[test]
fn measured_canvas_relayouts_only_when_its_height_changes() -> TestResult {
    let _rt = Runtime::new().enter();
    let content = signal("short".to_owned());
    let color = signal(Color::BLACK);
    let mut ui = TestUiBuilder::default().size(300.0, 600.0).build(move || {
        div().w(300.0).child(
            wrapped(content, color)
                .element(|canvas| canvas.test_id("wrapped"))
                .w_full(),
        )
    });
    ui.advance(0.0);
    let single = ui.bounds_of("wrapped").ok_or("canvas")?;

    let (layouts, _) = ui.passes();
    color.set(Color::RED);
    ui.advance(0.0);
    content.set("tiny".to_owned());
    ui.advance(0.0);
    assert_eq!(ui.passes().0, layouts, "same-height redraws need no layout");

    content.set(PARAGRAPH.to_owned());
    ui.advance(0.0);
    let grown = ui.bounds_of("wrapped").ok_or("canvas")?;
    assert!(ui.passes().0 > layouts);
    assert!(
        grown.size.height > single.size.height * 2.0,
        "{single:?} -> {grown:?}"
    );
    Ok(())
}

#[test]
fn wrapped_runs_mark_each_continuation_line_at_the_given_edge() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().size(300.0, 400.0).build(|| {
        canvas(|painter| {
            let width = painter.size().width;
            painter.run(
                TextRun::new(
                    PARAGRAPH,
                    Point::new(40.0, 0.0),
                    &TextStyle::default(),
                    Color::BLACK,
                )
                .wrap(width - 40.0)
                .continuation("+", 30.0, Color::RED)
                .name("body"),
            );
        })
        .w(240.0)
        .h(300.0)
    });
    ui.advance(0.0);
    let body = ui.bounds_of("body").ok_or("named run")?;
    let lines = (body.size.height / LINE).round() as usize;
    let marks = ui.bounds_of_all("body:continuation");
    assert!(lines > 2, "{body:?}");
    assert_eq!(marks.len(), lines - 1);
    for (index, mark) in marks.iter().enumerate() {
        assert!((mark.max().x - 30.0).abs() < 0.5, "{mark:?}");
        assert!(
            (mark.origin.y - LINE * (index + 1) as f32).abs() < 1.0,
            "{mark:?}"
        );
    }
    Ok(())
}

const PARAGRAPH: &str = "Measured canvases wrap their text runs to the width they are given \
    and report the wrapped height to layout, so rows can grow to fit long lines.";
const LINE: f32 = 14.0 * 1.4;
const PADDING: f32 = 4.0;

fn wrapped(content: Signal<String>, color: Signal<Color>) -> Canvas {
    let run = move |width: f32| {
        TextRun::new(
            content.get(),
            Point::new(0.0, PADDING / 2.0),
            &TextStyle::default(),
            color.get(),
        )
        .wrap(width)
        .tag(1)
        .name("body")
    };
    canvas(move |painter| painter.run(run(painter.size().width)))
        .measure(move |measure, width| measure.text(&run(width)).max().y + PADDING / 2.0)
}
