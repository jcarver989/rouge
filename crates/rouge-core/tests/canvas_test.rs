use rouge_core::testing::{BLUE, GREEN, RED, TestResult, TestUiBuilder, WHITE, assert_near};
use rouge_core::{
    Color, Path, Point, Rect, Styled, TextHit, TextRun, TextStyle, canvas, div, icon,
};
use rouge_reactive::{Runtime, signal};
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
