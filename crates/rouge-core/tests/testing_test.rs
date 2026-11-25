use rouge_core::testing::{
    BLUE, LINEAR, RED, TestError, TestResult, TestUi, TestUiBuilder, WHITE, assert_near,
};
use rouge_core::{
    Color, Keyframes, Menu, MenuItem, Point, PointerButton, Rect, Styled, canvas, div, dynamic,
    scroll,
};
use rouge_reactive::{Runtime, signal};
use std::assert_matches;
use std::path::PathBuf;

#[test]
fn the_builder_sets_the_window_size_scale_and_clear_color() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default()
        .size(40.0, 20.0)
        .scale(2.0)
        .clear(Color::BLUE)
        .build(|| div().size(10.0).bg(Color::RED));
    let img = ui.render()?;
    assert_eq!((img.width, img.height), (80, 40));
    assert_eq!((img.pixel(10, 10), img.pixel(30, 30)), (RED, BLUE));
    assert_eq!(ui.first_x(BLUE, 5)?, 20);
    assert_near(TestUi::canvas(div).pixel(1, 1)?, WHITE, "a canvas is white");
    Ok(())
}

#[test]
fn build_with_hands_back_what_the_root_closure_made() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, clicks) = TestUiBuilder::default().build_with(|| {
        let clicks = signal(0);
        let target = div()
            .size(50.0)
            .focusable()
            .on_click(move |_| clicks.update(|count| *count += 1));
        (div().size_full().child(target), clicks)
    });
    assert_eq!(ui.bounds(10.0, 10.0)?, Rect::new(0.0, 0.0, 50.0, 50.0));
    ui.click(Point::new(10.0, 10.0));
    ui.click_at(20.0, 20.0);
    assert_eq!(clicks.get_untracked(), 2);
    assert!(!ui.press("enter"), "nothing handles the key");
    assert_matches!(
        TestUi::new(div).bounds(500.0, 500.0),
        Err(TestError::NoElement { .. })
    );
    Ok(())
}

#[test]
fn run_and_wait_until_advance_the_clock_in_steps() -> TestResult {
    let _rt = Runtime::new().enter();
    let x = signal(0.0);
    let mut ui = TestUi::canvas(move || {
        let moving = div().size(20.0).bg(Color::RED).translate_x(x);
        div().size_full().child(moving.transition(LINEAR))
    });
    x.set(100.0);
    ui.run(0.5);
    assert_eq!(ui.first_x(RED, 10)?, 50);
    assert!(ui.wait_until(|ui| ui.first_x(RED, 10).is_ok_and(|x| x == 100)));
    assert_eq!(ui.draw()?.damaged, 0, "the settled frame is already drawn");
    ui.wait()?;
    Ok(())
}

#[test]
fn test_ids_follow_dynamic_replacements_and_keep_paint_order() -> TestResult {
    let _rt = Runtime::new().enter();
    let wide = signal(false);
    let clicks = signal(0);
    let mut ui = TestUi::new(|| {
        div()
            .child(dynamic(move || {
                let width = if wide.get() { 80.0 } else { 40.0 };
                div()
                    .test_id("row")
                    .w(width)
                    .h(20.0)
                    .on_click(move |_| clicks.update(|count| *count += 1))
            }))
            .child(div().test_id("row").size(20.0))
    });
    assert_eq!(ui.bounds_of("row"), Some(Rect::new(0.0, 0.0, 40.0, 20.0)));
    assert_eq!(
        ui.bounds_of_all("row"),
        vec![
            Rect::new(0.0, 0.0, 40.0, 20.0),
            Rect::new(0.0, 20.0, 20.0, 20.0)
        ]
    );
    ui.click_on("row")?;
    assert_eq!(clicks.get(), 1);
    wide.set(true);
    assert_eq!(ui.center_of("row")?, Point::new(40.0, 10.0));
    assert_eq!(ui.bounds_of_all("row").len(), 2);
    ui.hover_on("row")?;
    assert_matches!(ui.center_of("missing"), Err(TestError::NoTestId(_)));
    Ok(())
}

#[test]
fn visible_rect_of_distinguishes_clipped_content_from_missing_content() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        scroll()
            .test_id("scroller")
            .w(100.0)
            .h(45.0)
            .children(["first", "middle", "last"].map(|name| div().test_id(name).h(30.0)))
    });
    assert_eq!(ui.visible_rect_of("first")?, ui.rect_of("first")?);
    let middle = ui.rect_of("middle")?;
    let scroller = ui.rect_of("scroller")?;
    assert!(middle.origin.y < scroller.max().y && middle.max().y > scroller.max().y);
    assert_eq!(ui.visible_rect_of("middle")?, middle.intersect(&scroller));
    assert!(ui.visible_rect_of("last")?.is_empty());
    assert_matches!(ui.visible_rect_of("missing"), Err(TestError::NoTestId(_)));

    ui.scroll_into_view("scroller", "last")?;
    assert_eq!(ui.visible_rect_of("last")?, ui.rect_of("last")?);
    assert!(ui.visible_rect_of("first")?.is_empty());
    Ok(())
}

#[test]
fn visible_rect_of_clips_named_canvas_regions_to_the_window() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div().test_id("viewport").size_full().child(
            canvas(|painter| painter.region("region", Rect::new(20.0, 20.0, 40.0, 40.0)))
                .absolute()
                .right(0.0)
                .bottom(0.0)
                .size(40.0),
        )
    });
    let region = ui.rect_of("region")?;
    let viewport = ui.rect_of("viewport")?;
    let visible = ui.visible_rect_of("region")?;
    assert!(!visible.is_empty());
    assert_ne!(visible, region);
    assert_eq!(visible, region.intersect(&viewport));
    Ok(())
}

#[test]
fn drag_on_uses_the_public_drag_gesture() -> TestResult {
    let _rt = Runtime::new().enter();
    let moved = signal(Point::ZERO);
    let mut ui = TestUi::new(|| {
        div()
            .test_id("drag")
            .size(30.0)
            .on_drag(move |event| moved.set(event.translation))
    });
    ui.drag_on("drag", Point::new(100.0, 80.0))?;
    assert_eq!(moved.get(), Point::new(85.0, 65.0));
    Ok(())
}

#[test]
fn input_helpers_target_elements_by_test_id() -> TestResult {
    let _rt = Runtime::new().enter();
    let (saved, secondary, scrolled, dropped, second) = (
        signal(0),
        signal(0),
        signal(0.0),
        signal(Vec::new()),
        signal(false),
    );
    let menus = vec![Menu::new(
        "File",
        vec![MenuItem::action("Save", Save).shortcut("cmd-s")],
    )];
    let mut ui = TestUiBuilder::default()
        .size(100.0, 100.0)
        .menus(menus)
        .build(move || {
            let item = |index: usize| {
                div().test_id("item").size(10.0).on_click(move |_| {
                    if index == 1 {
                        second.set(true);
                    }
                })
            };
            div()
                .size_full()
                .on_action(move |_: &Save| saved.update(|saved| *saved += 1))
                .child(
                    div()
                        .test_id("target")
                        .size(40.0)
                        .on_pointer_down(move |event| {
                            if event.button == PointerButton::Secondary {
                                secondary.update(|count| *count += 1);
                            }
                        })
                        .on_scroll(move |event| scrolled.set(event.delta.y))
                        .on_file_drop(move |event| dropped.set(event.paths.clone())),
                )
                .child(item(0))
                .child(item(1))
        });
    ui.press("cmd-s cmd-s");
    ui.right_click_on("target")?;
    ui.scroll_on("target", Point::new(0.0, -5.0))?;
    ui.drop_files_on("target", vec!["/tmp/a".into()])?;
    ui.click_nth("item", 1)?;
    assert_eq!((saved.get(), secondary.get(), scrolled.get()), (2, 1, -5.0));
    assert_eq!(dropped.get(), [PathBuf::from("/tmp/a")]);
    assert!(second.get());
    Ok(())
}

#[derive(Clone, Copy)]
struct Save;

#[test]
fn a_frame_that_changes_no_pixels_is_drawn_but_not_presented() -> TestResult {
    let _rt = Runtime::new().enter();
    let color = signal(Color::RED);
    let mut ui = TestUi::new(move || div().size(10.0).bg(move || color.get()));
    assert_eq!(ui.draw()?.presented, 1, "the first frame is shown");
    assert_eq!(ui.draw()?.presented, 0, "an identical frame is not");
    color.set(Color::BLUE);
    ui.advance(0.0);
    assert_eq!(ui.draw()?.presented, 1, "a changed frame is shown again");
    assert_eq!(ui.pixel(5, 5)?, [0, 0, 255, 255]);
    Ok(())
}

#[test]
fn memory_reports_the_bytes_the_renderer_holds_for_the_window() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default()
        .size(100.0, 50.0)
        .scale(2.0)
        .build(|| div().size_full().bg(Color::RED));
    ui.draw()?;
    assert_eq!(ui.memory()?.scene, 200 * 100 * 4);
    Ok(())
}

#[test]
fn compositor_frames_counts_only_the_frames_the_compositor_wakes_for() -> TestResult {
    let _rt = Runtime::new().enter();
    let steps = (0..4u8).fold(Keyframes::new(), |keyframes, step| {
        let (start, end) = (f32::from(step) / 4.0, f32::from(step + 1) / 4.0);
        let x = f32::from(step) * 10.0;
        keyframes
            .at(start, |pose| pose.translate_x(x))
            .at(end, |pose| pose.translate_x(x))
    });
    let mut stepping = TestUi::new(move || {
        div()
            .size(10.0)
            .bg(Color::RED)
            .animate(steps.duration(1.0).repeat())
    });
    stepping.advance(0.0);
    stepping.draw()?;
    assert_eq!(
        stepping.compositor_frames(1.0)?,
        3,
        "a frame at each later step: 0.25, 0.5 and 0.75"
    );
    let mut idle = TestUi::new(|| div().size(10.0).bg(Color::RED));
    idle.draw()?;
    assert_eq!(idle.compositor_frames(1.0)?, 0);
    let slide = Keyframes::new()
        .at(0.0, |pose| pose.translate_x(0.0))
        .at(1.0, |pose| pose.translate_x(40.0));
    let mut sliding = TestUiBuilder::default()
        .refresh_rate(100.0)
        .build(move || div().size(10.0).bg(Color::RED).animate(slide.repeat()));
    sliding.draw()?;
    assert!(
        sliding.compositor_frames(1.0)? > 100,
        "a slide draws at the refresh rate"
    );
    Ok(())
}
