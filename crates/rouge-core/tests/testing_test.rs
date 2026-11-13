use rouge_core::testing::{
    BLUE, LINEAR, RED, TestError, TestResult, TestUi, TestUiBuilder, WHITE, assert_near,
};
use rouge_core::{Color, Key, Point, Rect, Styled, div};
use rouge_reactive::{Runtime, signal};

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
    assert!(!ui.press(Key::Enter), "nothing handles the key");
    assert!(matches!(
        TestUi::new(div).bounds(500.0, 500.0),
        Err(TestError::NoElement { .. })
    ));
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
    assert!(ui.draw()?.layers > 0);
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
            .child(rouge_core::dynamic(move || {
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
    assert!(matches!(
        ui.center_of("missing"),
        Err(TestError::NoTestId(_))
    ));
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
    let menus = vec![rouge_core::Menu::new(
        "File",
        vec![rouge_core::MenuItem::action("Save", Save).shortcut("cmd-s")],
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
                            if event.button == rouge_core::PointerButton::Secondary {
                                secondary.update(|count| *count += 1);
                            }
                        })
                        .on_scroll(move |event| scrolled.set(event.delta.y))
                        .on_file_drop(move |event| dropped.set(event.paths.clone())),
                )
                .child(item(0))
                .child(item(1))
        });
    ui.press_keys("cmd-s cmd-s");
    ui.right_click_on("target")?;
    ui.scroll_on("target", Point::new(0.0, -5.0))?;
    ui.drop_files_on("target", vec!["/tmp/a".into()])?;
    ui.click_nth("item", 1)?;
    assert_eq!((saved.get(), secondary.get(), scrolled.get()), (2, 1, -5.0));
    assert_eq!(dropped.get(), [std::path::PathBuf::from("/tmp/a")]);
    assert!(second.get());
    Ok(())
}

#[derive(Clone, Copy)]
struct Save;
