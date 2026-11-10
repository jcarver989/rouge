use rouge_core::testing::{BLUE, GREEN, RED, TestResult, TestUiBuilder, WHITE, assert_near};
use rouge_core::{Color, Path, Point, Rect, Styled, TextStyle, canvas, div, icon};
use rouge_reactive::{Runtime, signal};

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
