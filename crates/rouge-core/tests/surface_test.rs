use rouge_core::testing::{
    BLUE, GREEN, LINEAR, RED, TestResult, TestUi, TestUiBuilder, WHITE, assert_near,
};
use rouge_core::{
    Color, Pose, Rect, ScrollState, Scrollable, Styled, SurfaceError, SurfaceSource, SurfaceStatus,
    View, div, scroll, show, surface, text,
};
use rouge_reactive::{Runtime, signal};
use rouge_render::testing::{Pattern, TestSurface};
use rouge_render::{Point, SurfaceConfig};
use std::cell::Cell;
use std::error::Error;

#[test]
fn a_surface_is_laid_out_like_a_leaf_and_shows_its_producers_frames() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut producer, source) = TestSurface::new();
    let mut ui = TestUiBuilder::default()
        .size(100.0, 100.0)
        .scale(2.0)
        .build(|| div().p(10.0).child(surface(source).w(80.0).h(40.0)));
    ui.draw()?;
    producer.pump()?;
    assert_eq!(config(&producer)?, (160, 80, 2.0, true));
    producer.present(Pattern::quadrants([RED, GREEN, BLUE, WHITE]))?;
    let image = ui.render()?;
    assert_near(image.pixel(40, 40), RED, "top left");
    assert_near(image.pixel(160, 90), WHITE, "bottom right");
    assert_near(image.pixel(10, 10), [0, 0, 0, 0], "padding");
    assert_eq!(ui.bounds(50.0, 30.0)?, Rect::new(10.0, 10.0, 80.0, 40.0));
    Ok(())
}

#[test]
fn a_new_frame_costs_no_reactive_layout_or_paint_work() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut producer, mut ui) = filled(|source| surface(source).size(100.0))?;
    producer.present(Pattern::Solid(RED))?;
    ui.render()?;
    let before = ui.stats();
    producer.present(Pattern::Solid(BLUE))?;
    ui.advance(0.016);
    assert_near(ui.pixel(50, 50)?, BLUE, "the new frame");
    let after = ui.stats();
    assert_eq!(
        (after.layouts, after.paints, after.reactive.effects_run),
        (before.layouts, before.paints, before.reactive.effects_run)
    );
    Ok(())
}

#[test]
fn resizing_reconfigures_the_producer_and_stretches_the_old_frame() -> TestResult {
    let _rt = Runtime::new().enter();
    let width = signal(50.0);
    let (mut producer, mut ui) =
        filled(move |source| surface(source).w(move || width.get()).h(100.0))?;
    producer.present(Pattern::quadrants([RED, GREEN, BLUE, WHITE]))?;
    ui.render()?;
    width.set(100.0);
    ui.advance(0.0);
    let image = ui.render()?;
    assert_near(image.pixel(75, 25), GREEN, "the old frame stretches");
    producer.pump()?;
    assert_eq!(config(&producer)?, (100, 100, 1.0, true));
    Ok(())
}

#[test]
fn hidden_clipped_and_occluded_surfaces_are_not_visible() -> TestResult {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let (mut producer, mut ui) = filled(move |source| {
        scroll()
            .size(100.0)
            .scroll_state(state)
            .child(div().h(100.0))
            .child(surface(source).size(100.0))
    })?;
    assert!(!config(&producer)?.3, "scrolled out of view");
    state.update(|s| s.scroll_to(Point::new(0.0, 100.0)));
    ui.advance(0.0);
    ui.draw()?;
    producer.pump()?;
    assert!(config(&producer)?.3, "scrolled into view");
    ui.set_visible(false);
    ui.draw()?;
    producer.pump()?;
    assert!(!config(&producer)?.3, "the window is occluded");
    let (invisible, _ui) = filled(|source| surface(source).size(100.0).invisible())?;
    assert!(!config(&invisible)?.3, "the element is invisible");
    Ok(())
}

#[test]
fn a_failed_surface_shows_its_fallback_and_reports_its_status() -> TestResult {
    let _rt = Runtime::new().enter();
    let status = signal(SurfaceStatus::Waiting);
    let (mut producer, mut ui) = filled(move |source| {
        surface(source)
            .size(100.0)
            .status(status)
            .fallback(|error| div().test_id("fallback").child(text(error.to_string())))
    })?;
    producer.present(Pattern::Solid(RED))?;
    ui.draw()?;
    ui.advance(0.0);
    assert_eq!(status.get_untracked(), SurfaceStatus::Live);
    assert!(ui.rect_of("fallback").is_err());
    producer.fail(SurfaceError::Producer("no GPU".into()));
    ui.draw()?;
    ui.advance(0.0);
    assert_eq!(
        status.get_untracked(),
        SurfaceStatus::Failed(SurfaceError::Producer("no GPU".into()))
    );
    assert!(ui.rect_of("fallback").is_ok());
    Ok(())
}

#[test]
fn removing_a_surface_detaches_the_producer_after_its_exit() -> TestResult {
    let _rt = Runtime::new().enter();
    let open = signal(true);
    let (mut producer, mut ui) = filled(move |source| {
        let source = Cell::new(Some(source));
        show(
            move || open.get(),
            move || source.take().map(|source| surface(source).size(100.0)),
        )
        .exit(Pose::new().opacity(0.0), LINEAR)
    })?;
    producer.present(Pattern::Solid(RED))?;
    ui.render()?;
    open.set(false);
    ui.advance(0.0);
    ui.draw()?;
    producer.pump()?;
    assert!(producer.is_detached());
    ui.advance(0.5);
    assert!(ui.pixel(50, 50)?[0] > 0, "the last frame fades out");
    assert_eq!(ui.surfaces()?.stats().surfaces, 1);
    ui.run(1.0);
    ui.draw()?;
    assert_eq!(ui.surfaces()?.stats().surfaces, 0);
    Ok(())
}

fn filled<T: View>(
    view: impl FnOnce(SurfaceSource) -> T,
) -> Result<(TestSurface, TestUi), Box<dyn Error>> {
    let (mut producer, source) = TestSurface::new();
    let mut ui = TestUiBuilder::default()
        .size(100.0, 100.0)
        .clear(Color::WHITE)
        .build(|| view(source));
    ui.draw()?;
    producer.pump()?;
    Ok((producer, ui))
}

fn config(producer: &TestSurface) -> Result<(u32, u32, f32, bool), &'static str> {
    let SurfaceConfig {
        width,
        height,
        scale,
        visible,
        ..
    } = producer
        .config()
        .ok_or("the producer was never configured")?;
    Ok((width, height, scale, visible))
}
