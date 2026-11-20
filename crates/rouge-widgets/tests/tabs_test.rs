mod common;

use common::{builder, handheld_test};
use rouge_core::testing::{TestResult, TestUi, assert_near};
use rouge_core::view;
use rouge_core::{Color, InputKind, Size, Styled, UiPalette, div};
use rouge_reactive::{Runtime, Signal, on_cleanup, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{IconName, tabs};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn tabs_mount_only_the_selected_panel_and_switch_by_click_and_arrows() -> TestResult {
    let _entered = Runtime::new().enter();
    let selected = signal(0usize);
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, second) = (log.clone(), log.clone());
    let mut ui = builder().build(move || {
        view! {
            <tabs selected=selected test_id="tabs"
                 tab=("General", move || {
                     first.borrow_mut().push("mount general");
                     let log = first.clone();
                     on_cleanup(move || log.borrow_mut().push("dispose general"));
                     view! { <div test_id="general" w=120.0 h=40.0/> }
                 })
                 tab=("Advanced", move || {
                     second.borrow_mut().push("mount advanced");
                     view! { <div test_id="advanced" w=60.0 h=40.0/> }
                 })
                 size_full/>
        }
    });
    assert_eq!(*log.borrow(), ["mount general"]);
    assert_eq!(ui.rect_of("general")?.size.width, 120.0);
    ui.click_on("tabs-tab-1")?;
    assert_eq!(selected.get(), 1);
    assert_eq!(
        *log.borrow(),
        ["mount general", "mount advanced", "dispose general"],
        "the outgoing panel is disposed in the same update and only fades as an orphan layer"
    );
    assert_eq!(ui.rect_of("advanced")?.size.width, 60.0);
    ui.press("right");
    assert_eq!(selected.get(), 0, "arrows wrap around");
    ui.press("left");
    assert_eq!(selected.get(), 1);
    Ok(())
}

#[test]
fn one_underline_slides_to_the_selected_tab_and_then_idles() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, selected) = three_tabs();
    ui.run(0.5);
    let created = ui.stats().elements_created;
    let first = ui.rect_of("tabs-tab-0")?;
    let under = ui.rect_of("tabs-indicator")?;
    assert_eq!(
        (under.origin.x, under.size.width, under.size.height),
        (first.origin.x, first.size.width, 2.0)
    );
    assert_eq!(under.max().y, first.max().y, "it sits on the strip's edge");
    selected.set(2);
    ui.advance(0.0);
    assert!(ui.is_animating(), "the underline slides");
    ui.run(1.0);
    let third = ui.rect_of("tabs-tab-2")?;
    let under = ui.rect_of("tabs-indicator")?;
    assert_eq!(
        (under.origin.x, under.size.width),
        (third.origin.x, third.size.width)
    );
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    assert!(
        ui.stats().elements_created <= created + 1,
        "only the incoming panel is built"
    );
    Ok(())
}

#[test]
fn tab_reaches_a_header_and_draws_the_accent_focus_border() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, _) = three_tabs();
    ui.run(0.5);
    ui.press("tab");
    assert!(ui.has_focus());
    ui.run(0.5);
    let at = ui.rect_of("tabs-tab-0")?;
    let accent = UiPalette::light().map(Color::from).accent.to_rgba8();
    assert_near(
        ui.pixel(at.origin.x as u32, (at.origin.y + 12.0) as u32)?,
        accent,
        "focus border",
    );
    Ok(())
}

#[test]
fn tabs_inside_a_page_keep_their_strip_on_a_handheld_test() -> TestResult {
    let _entered = Runtime::new().enter();
    let selected = signal(0usize);
    let mut ui = handheld_test().build(move || {
        view! {
            <tabs selected=selected test_id="tabs" tab=("Tracks", || view! { <div h=40.0/> })
                 tab=("Credits", || view! { <div h=40.0/> })/>
        }
    });
    ui.run(1.0);
    assert_eq!(ui.bounds_of("tabs-bar"), None, "the bar is opt-in");
    assert_eq!(ui.rect_of("tabs-tab-0")?.origin.y, 0.0);
    Ok(())
}

#[test]
fn tabs_present_a_floating_bar_on_a_handheld_and_a_strip_otherwise() -> TestResult {
    let _entered = Runtime::new().enter();
    let selected = signal(0usize);
    let mut ui = handheld_test().build(move || {
        view! {
            <tabs selected=selected test_id="tabs" bar
                 icon_tab=(IconName::Folder, "Library", || view! { <div test_id="library"/> })
                 tab=("Search", || view! { <div test_id="search"/> }) size_full/>
        }
    });
    ui.run(1.0);
    let bar = ui.rect_of("tabs-bar")?;
    assert_eq!(
        (bar.origin.x, bar.max().y, bar.size.width),
        (12.0, 688.0, 366.0),
        "the bar floats above the bottom edge"
    );
    let tab = ui.rect_of("tabs-tab-1")?;
    assert!(tab.size.height >= 44.0, "tabs are touch sized: {tab:?}");
    ui.tap_on("tabs-tab-1")?;
    assert_eq!(selected.get(), 1);
    ui.run(1.0);
    let pill = ui.rect_of("tabs-indicator")?;
    assert_eq!(pill, tab, "one highlight slides under the selected tab");
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    ui.set_size(Size::new(900.0, 700.0));
    ui.run(1.0);
    assert_eq!(ui.bounds_of("tabs-bar"), None, "a tablet has no bottom bar");
    let rail = ui.rect_of("tabs-rail")?;
    assert_eq!(
        (rail.origin.x, rail.size.width, rail.size.height),
        (0.0, 80.0, 700.0),
        "the same tabs stand in a rail at the leading edge"
    );
    let (first, second) = (ui.rect_of("tabs-tab-0")?, ui.rect_of("tabs-tab-1")?);
    assert!(
        second.origin.y >= first.max().y,
        "stacked: {first:?} {second:?}"
    );
    assert_eq!(ui.rect_of("tabs-indicator")?, second);
    assert_eq!(
        ui.rect_of("search")?.origin.x,
        80.0,
        "the panel sits beside it"
    );
    ui.set_input(InputKind::Pointer);
    ui.run(1.0);
    assert_eq!(ui.bounds_of("tabs-rail"), None, "a pointer keeps the strip");
    assert_eq!(ui.rect_of("tabs-tab-0")?.origin.y, 0.0);
    assert!(ui.bounds_of("search").is_some(), "the panel stays mounted");
    Ok(())
}

#[test]
fn a_tab_counts_in_a_badge_that_caps_and_hides_at_zero_or_shows_a_bare_dot() -> TestResult {
    let _entered = Runtime::new().enter();
    let (selected, unread, fresh) = (signal(0usize), signal(3usize), signal(true));
    let mut ui = handheld_test().build(move || {
        view! {
            <tabs selected=selected test_id="tabs" bar
                 icon_tab=(IconName::Folder, "Inbox", || view! { <div/> }) badge=unread
                 icon_tab=(IconName::Bell, "Activity", || view! { <div/> }) dot=fresh
                 tab=("You", || view! { <div/> }) size_full/>
        }
    });
    ui.run(1.0);
    let (tab, badge) = (ui.rect_of("tabs-tab-0")?, ui.rect_of("tabs-tab-0-badge")?);
    assert_eq!(badge.size.height, 16.0);
    assert!(
        badge.origin.x > tab.center().x && badge.origin.y < tab.center().y,
        "the count sits at the icon's top trailing corner: {badge:?} in {tab:?}"
    );
    assert!(ui.text_bounds("3").is_some());
    unread.set(250);
    ui.run(0.5);
    assert!(ui.text_bounds("99+").is_some(), "large counts are capped");
    unread.set(0);
    ui.run(0.5);
    assert_eq!(ui.bounds_of("tabs-tab-0-badge"), None, "zero draws nothing");
    let dot = ui.rect_of("tabs-tab-1-badge")?;
    assert_eq!((dot.size.width, dot.size.height), (8.0, 8.0));
    fresh.set(false);
    ui.run(0.5);
    assert_eq!(ui.bounds_of("tabs-tab-1-badge"), None);
    assert_eq!(ui.bounds_of("tabs-tab-2-badge"), None, "badges are opt-in");
    Ok(())
}

#[test]
fn the_bar_carries_an_accessory_above_its_tabs_and_hides_under_the_keyboard() -> TestResult {
    let _entered = Runtime::new().enter();
    let selected = signal(0usize);
    let mut ui = handheld_test().build(move || {
        view! {
            <tabs selected=selected test_id="tabs" bar
                 accessory={|| view! { <div test_id="upload" h=20.0 w=100.0/> }}
                 tab=("Inbox", || view! { <div/> }) tab=("You", || view! { <div/> }) size_full/>
        }
    });
    ui.run(1.0);
    let (bar, slot, tab) = (
        ui.rect_of("tabs-bar")?,
        ui.rect_of("tabs-accessory")?,
        ui.rect_of("tabs-tab-0")?,
    );
    assert_eq!(bar.max().y, 688.0, "the bar still floats above the edge");
    assert!(slot.size.height >= 44.0 && slot.max().y <= tab.origin.y);
    assert!(slot.origin.y >= bar.origin.y && ui.bounds_of("upload").is_some());
    assert_eq!(
        ui.rect_of("tabs-indicator")?,
        tab,
        "the highlight still finds its tab"
    );
    ui.set_keyboard_inset(300.0);
    ui.run(0.5);
    assert_eq!(
        ui.bounds_of("tabs-bar"),
        None,
        "the keyboard takes the bar's place"
    );
    ui.set_keyboard_inset(0.0);
    ui.run(0.5);
    assert!(ui.bounds_of("tabs-bar").is_some());
    Ok(())
}

fn three_tabs() -> (TestUi, Signal<usize>) {
    let selected = signal(0usize);
    let ui = component_test()
        .size(320.0, 120.0)
        .clear(Color::WHITE)
        .build(move || {
            view! {
                <tabs selected=selected test_id="tabs" tab=("Tracks", || view! { <div h=20.0/> })
                     tab=("Credits", || view! { <div h=20.0/> }) tab=("Notes", || view! { <div h=20.0/> })
                     size_full/>
            }
        });
    (ui, selected)
}
