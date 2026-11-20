mod common;

use common::handheld_test;
use rouge_core::testing::{TestError, TestResult, TestUi};
use rouge_core::view;
use rouge_core::{Point, ScrollState, Scrollable, Styled, div, scroll};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{IconName, icon_button, nav_bar};

#[test]
fn the_bar_centres_its_title_between_a_leading_slot_and_trailing_actions() -> TestResult {
    let _entered = Runtime::new().enter();
    let searched = signal(0);
    let mut ui = handheld_test().build(move || {
        view! {
            <div flex col size_full>
                <nav_bar title="Inbox" test_id="bar"
                     leading={icon_button(IconName::PanelLeft, "Menu".to_owned()).test_id("menu")}
                     trailing={icon_button(IconName::Search, "Search".to_owned())
                         .test_id("search")
                         .on_click(move || searched.update(|count| *count += 1))}
                     trailing={icon_button(IconName::Ellipsis, "More".to_owned()).test_id("more")}/>
            </div>
        }
    });
    ui.run(0.5);
    let (bar, title) = (ui.rect_of("bar")?, ui.text_bounds("Inbox").ok_or("title")?);
    assert_eq!(
        (bar.origin.y, bar.size.width, bar.size.height),
        (0.0, 390.0, 45.0)
    );
    assert!(
        (title.center().x - 195.0).abs() <= 1.0,
        "the title is centred in the window: {title:?}"
    );
    let (menu, search, more) = (
        ui.rect_of("menu")?,
        ui.rect_of("search")?,
        ui.rect_of("more")?,
    );
    assert_eq!((menu.origin.x, menu.size.width), (4.0, 44.0));
    assert!(search.max().x <= more.origin.x);
    assert_eq!(more.max().x, 386.0);
    ui.tap_on("search")?;
    assert_eq!(searched.get(), 1);
    Ok(())
}

#[test]
fn a_large_title_folds_into_the_bar_once_the_scroller_moves_and_returns_at_the_top() -> TestResult {
    let _entered = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = handheld_test().build(move || {
        let rows = (0..40).map(|_| view! { <div flex_none h=44.0/> });
        view! {
            <div flex col size_full>
                <nav_bar title="Inbox" test_id="bar" large scroll=state/>
                {scroll().flex().col().children(rows).scroll_state(state).test_id("list").flex_1().min_h(0.0)}
            </div>
        }
    });
    ui.run(0.5);
    let open = ui.rect_of("bar")?;
    assert_eq!(open.size.height, 85.0, "a 44 px bar over a 40 px title row");
    assert_eq!(ui.rect_of("bar-large")?.size.height, 40.0);
    assert!(!inked(&mut ui, "bar-title")?, "the inline title waits");
    ui.scroll_on("list", Point::new(0.0, -120.0))?;
    ui.run(1.0);
    assert_eq!(ui.rect_of("bar")?.size.height, 45.0, "the row folds away");
    assert!(inked(&mut ui, "bar-title")?, "and the bar takes the title");
    ui.scroll_on("list", Point::new(0.0, 400.0))?;
    ui.run(1.0);
    assert_eq!(
        ui.rect_of("bar")?.size.height,
        85.0,
        "back at the top it returns"
    );
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

fn inked(ui: &mut TestUi, name: &str) -> Result<bool, TestError> {
    let (at, image) = (ui.rect_of(name)?, ui.render()?);
    let paper = image.pixel(at.origin.x as u32 + 1, at.origin.y as u32 + 1);
    let (y, centre) = (at.center().y as u32, at.center().x as u32);
    Ok((centre - 30..centre + 30).any(|x| {
        (y - 6..y + 6).any(|y| {
            image
                .pixel(x, y)
                .iter()
                .zip(paper)
                .any(|(a, b)| a.abs_diff(b) > 40)
        })
    }))
}
