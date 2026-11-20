use rouge_core::testing::{TestResult, TestUi};
use rouge_core::view;
use rouge_core::{InputKind, Point, Styled, div};
use rouge_reactive::{Runtime, Signal, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::Space;
use rouge_widgets::{page_dots, pager};

#[test]
fn pages_snap_under_a_swipe_one_at_a_time_and_stop_at_the_ends() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, page) = cards();
    ui.run(0.5);
    let (first, second) = (ui.rect_of("cards-page-0")?, ui.rect_of("cards-page-1")?);
    assert_eq!((first.origin.x, first.size.width), (24.0, 342.0));
    assert_eq!(
        second.origin.x,
        first.max().x + 12.0,
        "the next page peeks in"
    );
    ui.swipe_over(Point::new(300.0, 80.0), Point::new(120.0, 80.0), 0.2);
    assert_eq!(page.get(), 1);
    ui.run(1.0);
    assert_eq!(
        ui.rect_of("cards-page-1")?.origin.x,
        24.0,
        "it settles in place"
    );
    ui.swipe_over(Point::new(200.0, 80.0), Point::new(170.0, 80.0), 0.4);
    ui.run(1.0);
    assert_eq!(page.get(), 1, "a short drag springs back");
    ui.swipe_over(Point::new(380.0, 80.0), Point::new(10.0, 80.0), 0.05);
    ui.run(1.0);
    assert_eq!(page.get(), 2, "even a fast fling moves one page");
    ui.swipe_over(Point::new(300.0, 80.0), Point::new(60.0, 80.0), 0.2);
    ui.run(1.0);
    assert_eq!(page.get(), 2, "the last page is the end");
    assert_eq!(ui.rect_of("cards-page-2")?.origin.x, 24.0);
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

#[test]
fn the_dots_follow_the_page_and_set_it() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, page) = cards();
    ui.run(0.5);
    let dots = ui.rect_of("dots")?;
    assert_eq!(dots.size.height, 24.0);
    let wide = |ui: &mut TestUi| -> Vec<f32> {
        ui.bounds_of_all("page-dot")
            .iter()
            .map(|dot| dot.size.width)
            .collect()
    };
    assert_eq!(
        wide(&mut ui),
        [20.0, 8.0, 8.0],
        "the current page's dot is a pill"
    );
    page.set(2);
    ui.run(1.0);
    assert_eq!(wide(&mut ui), [8.0, 8.0, 20.0]);
    assert_eq!(
        ui.rect_of("cards-page-2")?.origin.x,
        24.0,
        "the pager follows the signal"
    );
    let second = ui.bounds_of_all("page-dot")[1].center();
    ui.tap(second);
    assert_eq!(page.get(), 1, "a dot is a way to its page");
    ui.click_on("cards")?;
    ui.press("right");
    assert_eq!(page.get(), 2, "arrows turn the focused pager");
    Ok(())
}

fn cards() -> (TestUi, Signal<usize>) {
    let page = signal(0usize);
    let ui = component_test()
        .size(390.0, 300.0)
        .input(InputKind::Touch)
        .build(move || {
            let pages = (0..3).map(|_| view! { <div h=148.0/> });
            view! {
                <div flex col size_full>
                    {pager(page).peek(Space::X6).children(pages).test_id("cards")}
                    <div test_id="dots" flex justify_center>{page_dots(page, 3)}</div>
                </div>
            }
        });
    (ui, page)
}
