use rouge_core::testing::{TestError, TestResult, TestUi};
use rouge_core::{
    Corners, FocusHandle, InputKind, Rect, Size, Styled, View, div, focus_handle, view,
};
use rouge_reactive::{Runtime, signal};
use rouge_webview::testing::FakeEngine;
use rouge_webview::{PageCommand, WebContent, WebPage, WebViewError, WebViewEvent, web_page};
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::MotionScale;
use rouge_widgets::web_card;

const PRICING_HTML: &str = "<h1>Pricing</h1>";

#[test]
fn the_page_fills_the_card_below_its_header_scaled_to_the_card_width() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let mut ui = stage(&engine, || {
        view! {
            <div p=20.0>
                <web_card page={pricing()} name="pricing.html" tag="v3" page_size=(960.0, 600.0) w=482.0
                     test_id="card"/>
            </div>
        }
    })?;
    let (card, viewport) = (ui.rect_of("card")?, ui.rect_of("card-viewport")?);
    assert_eq!(card.origin.x, 20.0);
    assert_eq!(viewport.size.width, 480.0, "inside the card's hairline");
    assert_eq!(viewport.size.height, 300.0, "the page's 16:10 shape");
    assert!(
        viewport.origin.y >= card.origin.y + 40.0,
        "below the header"
    );
    assert!(ui.text_bounds("pricing.html").is_some() && ui.text_bounds("v3").is_some());
    let placed = engine.placement(0).ok_or("placement")?;
    assert_eq!(placed.frame, viewport);
    assert_eq!(placed.page_zoom, 0.5, "a 960 page in 480 points");
    assert_eq!(
        placed.corners,
        Corners {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: 11.0,
            bottom_left: 11.0,
        }
    );
    assert_eq!(engine.content(0), Some(WebContent::html(PRICING_HTML)));
    Ok(())
}

#[test]
fn a_card_sleeps_until_it_is_clicked_and_sleeps_again_on_a_click_outside() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let (mut ui, focus) = card(&engine)?;
    assert!(!engine.placement(0).ok_or("asleep")?.interactive);
    assert!(ui.text_bounds("Click to interact").is_some());
    ui.click_on("card-viewport")?;
    ui.wait_for_idle()?;
    assert!(focus.is_focused());
    assert!(engine.placement(0).ok_or("awake")?.interactive);
    assert!(ui.text_bounds("Click outside to return").is_some());
    ui.click_on("elsewhere")?;
    ui.wait_for_idle()?;
    assert!(!focus.is_focused());
    assert!(!engine.placement(0).ok_or("asleep again")?.interactive);
    Ok(())
}

#[test]
fn escape_and_the_owner_can_both_put_an_awake_card_to_sleep() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let (mut ui, focus) = card(&engine)?;
    focus.focus();
    ui.wait_for_idle()?;
    assert!(engine.placement(0).ok_or("woken by its owner")?.interactive);
    ui.press("escape");
    ui.wait_for_idle()?;
    assert!(!focus.is_focused(), "Escape returns to the thread");
    focus.focus();
    ui.wait_for_idle()?;
    assert!(engine.placement(0).ok_or("woken")?.interactive);
    focus.blur();
    ui.wait_for_idle()?;
    assert!(!engine.placement(0).ok_or("slept")?.interactive);
    Ok(())
}

#[test]
fn the_header_reloads_the_page_and_opens_it_elsewhere() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let opened = signal(0);
    let mut ui = stage(&engine, move || {
        view! {
            <web_card page={web_page(WebContent::url("https://example.com/"))} name="example.com"
                 on_open={move || opened.update(|count| *count += 1)} w=480.0 test_id="card"/>
        }
    })?;
    ui.click_on("card-reload")?;
    ui.click_on("card-open")?;
    ui.wait_for_idle()?;
    assert!(engine.calls(0).contains(&PageCommand::Reload));
    assert_eq!(opened.get(), 1);
    Ok(())
}

#[test]
fn an_expandable_card_opens_when_pressed_instead_of_waking() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let expanded = signal(0);
    let mut ui = stage(&engine, move || {
        view! {
            <web_card page={pricing()} name="pricing.html"
                 on_expand={move || expanded.update(|count| *count += 1)} w=480.0 test_id="card"/>
        }
    })?;
    assert!(ui.text_bounds("Click to open").is_some());
    ui.click_on("card-viewport")?;
    ui.wait_for_idle()?;
    assert_eq!(expanded.get(), 1);
    assert!(
        !engine.placement(0).ok_or("placed")?.interactive,
        "the page woke in place"
    );
    ui.click_on("card-expand")?;
    ui.wait_for_idle()?;
    assert_eq!(expanded.get(), 2);
    Ok(())
}

#[test]
fn an_expandable_card_given_a_focus_handle_still_expands() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let expanded = signal(0);
    let (mut ui, focus) = component_test().size(800.0, 600.0).setup(engine.install()).build_with(|| {
            let focus = focus_handle();
            let view = view! {
                <web_card page={pricing()} name="pricing.html"
                     on_expand={move || expanded.update(|count| *count += 1)} track_focus=focus w=480.0
                     test_id="card"/>
            };
            (view, focus)
    });
    ui.click_on("card-viewport")?;
    focus.focus();
    ui.wait_for_idle()?;
    assert_eq!(expanded.get(), 1);
    assert!(!focus.is_focused(), "an expanding card took focus");
    assert!(!engine.placement(0).ok_or("placed")?.interactive);
    Ok(())
}

#[test]
fn a_card_with_nowhere_to_open_has_no_open_button() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let (ui, _focus) = card(&engine)?;
    assert!(ui.bounds_of("card-reload").is_some());
    assert!(ui.bounds_of("card-expand").is_some());
    assert_eq!(ui.bounds_of("card-open"), None);
    Ok(())
}

#[test]
fn a_card_opens_its_page_full_screen_and_brings_it_back() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let mut ui = stage(&engine, || {
        view! {
            <div flex col gap=20.0>
                <web_card page={pricing()} name="pricing.html" w=480.0 test_id="card"/>
                <web_card page={pricing()} name="other.html" w=480.0 test_id="other"/>
            </div>
        }
    })?;
    assert!(engine.placement(1).ok_or("the other card")?.visible());
    let inline = engine.placement(0).ok_or("in the card")?;
    assert!(!inline.interactive);
    ui.click_on("card-expand")?;
    ui.advance_frames(1);
    assert!(!engine.placement(1).ok_or("covered card")?.visible());
    ui.wait_for_idle()?;
    let (screen, full) = (
        ui.rect_of("card-screen")?,
        engine.placement(0).ok_or("full screen")?,
    );
    assert_eq!(screen, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_eq!(full.frame, ui.rect_of("card-screen-page")?);
    assert_eq!(full.frame.size.width, 800.0);
    assert_eq!(full.frame.max().y, 600.0);
    assert!(full.frame.origin.y >= 40.0, "below the full-screen header");
    assert_eq!(full.page_zoom, 1.0, "the page at its own size");
    assert_eq!(full.corners, Corners::all(0.0));
    assert!(full.interactive);
    ui.click_on("card-collapse")?;
    ui.wait_for_idle()?;
    assert_eq!(ui.bounds_of("card-screen"), None);
    assert_eq!(engine.placement(0), Some(inline), "back in the card");
    assert_eq!(engine.opened(), 2);
    assert!(engine.placement(1).ok_or("restored card")?.visible());
    Ok(())
}

#[test]
fn the_page_grows_out_of_the_card_without_changing_its_layout_size() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let (mut ui, _focus) = card(&engine)?;
    let inline = engine.placement(0).ok_or("in the card")?;
    engine.take_calls(0);
    ui.click_on("card-expand")?;
    ui.advance_frames(1);
    let growing = engine.placement(0).ok_or("growing")?;
    let (from, shown) = (inline.clip.size, growing.clip.size);
    assert!(
        shown.width > from.width && shown.width < 800.0,
        "part of the way: {shown:?}"
    );
    assert!(shown.height > from.height && growing.clip.max().y < 600.0);
    ui.wait_for_idle()?;
    let full = ui.rect_of("card-screen-page")?.size;
    let sizes: Vec<Size> = engine
        .calls(0)
        .iter()
        .filter_map(|call| match call {
            PageCommand::Place(placed) => placed.layout(1.0),
            _ => None,
        })
        .map(|layout| layout.unzoomed)
        .collect();
    assert!(sizes.len() > 10, "the page is placed on every frame");
    assert!(
        sizes.iter().all(|size| *size == full),
        "the web view was resized while it grew: {sizes:?}"
    );
    Ok(())
}

#[test]
fn leaving_full_screen_part_of_the_way_there_returns_from_where_the_page_is() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let (mut ui, _focus) = card(&engine)?;
    let inline = engine.placement(0).ok_or("in the card")?;
    ui.click_on("card-expand")?;
    ui.advance_frames(1);
    let turned = engine.placement(0).ok_or("growing")?.clip.size.width;
    ui.press("escape");
    ui.advance_frames(1);
    let returning = engine.placement(0).ok_or("returning")?.clip.size.width;
    assert!(
        returning > inline.clip.size.width && returning < turned + 80.0,
        "it turned around at {turned} and is now {returning}"
    );
    ui.wait_for_idle()?;
    assert_eq!(
        ui.bounds_of("card-screen"),
        None,
        "Escape leaves full screen"
    );
    assert_eq!(engine.placement(0), Some(inline));
    ui.click_on("card-expand")?;
    ui.click_on("card-collapse")?;
    ui.click_on("card-expand")?;
    ui.wait_for_idle()?;
    assert_eq!(
        engine.placement(0).ok_or("full again")?.frame.size.width,
        800.0,
        "it opens again once it has closed"
    );
    assert!(ui.bounds_of("card-screen").is_some());
    Ok(())
}

#[test]
fn with_motion_off_the_page_is_full_screen_at_once() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let mut ui = stage(&engine, || {
        MotionScale::None.provide();
        view! { <web_card page={pricing()} name="pricing.html" w=480.0 test_id="card"/> }
    })?;
    ui.click_on("card-expand")?;
    ui.advance_frames(1);
    assert_eq!(
        engine.placement(0).ok_or("full screen")?.frame.size.width,
        800.0
    );
    ui.click_on("card-collapse")?;
    ui.advance_frames(1);
    assert_eq!(ui.bounds_of("card-screen"), None);
    Ok(())
}

#[test]
fn a_tooltip_beside_a_page_opens_clear_of_it() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let mut ui = stage(&engine, || {
        view! {
            <div pt=120.0 pl=120.0>
                <web_card page={pricing()} name="pricing.html" w=480.0 test_id="card"/>
            </div>
        }
    })?;
    ui.hover_on("card-expand")?;
    assert!(ui.wait_until(|ui| ui.text_bounds("Full screen").is_some()));
    ui.wait_for_idle()?;
    let (tip, page) = (
        ui.text_bounds("Full screen").ok_or("no tooltip")?,
        engine.placement(0).ok_or("placed")?.clip,
    );
    assert!(
        tip.intersect(&page).is_empty(),
        "the tooltip at {tip:?} is under the page at {page:?}"
    );
    assert!(
        tip.max().y <= ui.rect_of("card-expand")?.origin.y,
        "above its button"
    );
    ui.click_on("card-expand")?;
    ui.hover_on("card-collapse")?;
    assert!(ui.wait_until(|ui| ui.text_bounds("Exit full screen").is_some()));
    ui.wait_for_idle()?;
    let (tip, page) = (
        ui.text_bounds("Exit full screen").ok_or("no tooltip")?,
        engine.placement(0).ok_or("full screen")?.clip,
    );
    assert!(
        tip.intersect(&page).is_empty(),
        "the tooltip at {tip:?} is under the full-screen page at {page:?}"
    );
    assert!(
        tip.max().x <= ui.rect_of("card-collapse")?.origin.x,
        "beside its button, with no room above"
    );
    Ok(())
}

#[test]
fn the_header_says_what_the_page_is_doing() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let (mut ui, _) = card(&engine)?;
    engine.emit(0, WebViewEvent::Loading(true));
    ui.wait_for_idle()?;
    assert_eq!(ui.text_of("card-status").as_deref(), Some("Loading"));
    engine.emit(0, WebViewEvent::Failed("The server is down.".into()));
    ui.wait_for_idle()?;
    assert_eq!(
        ui.text_of("card-status").as_deref(),
        Some("Couldn't load: The server is down.")
    );
    engine.emit(0, WebViewEvent::Loading(true));
    engine.emit(0, WebViewEvent::Loading(false));
    ui.wait_for_idle()?;
    assert_eq!(
        ui.text_of("card-status").as_deref(),
        Some("Click to interact")
    );
    Ok(())
}

#[test]
fn a_platform_without_web_views_gets_a_card_that_says_so() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new().failing(WebViewError::Unsupported);
    let (ui, focus) = card(&engine)?;
    assert_eq!(
        ui.text_of("card-status").as_deref(),
        Some("Unavailable here")
    );
    assert!(ui.text_bounds("No web view here").is_some());
    assert!(
        ui.text_bounds("no system web view is available on this platform")
            .is_some()
    );
    assert!(!focus.is_focused());
    Ok(())
}

#[test]
fn a_touch_card_asks_for_a_tap() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let mut ui = component_test()
        .size(390.0, 700.0)
        .input(InputKind::Touch)
        .setup(engine.install())
        .build(|| view! { <web_card page={pricing()} name="pricing.html" test_id="card"/> });
    ui.wait_for_idle()?;
    assert_eq!(
        ui.text_of("card-status").as_deref(),
        Some("Tap to interact")
    );
    ui.tap_on("card-viewport")?;
    ui.wait_for_idle()?;
    assert!(engine.placement(0).ok_or("awake")?.interactive);
    assert_eq!(
        ui.text_of("card-status").as_deref(),
        Some("Tap outside to return")
    );
    Ok(())
}

fn pricing() -> WebPage {
    web_page(WebContent::html(PRICING_HTML))
}

fn card(engine: &FakeEngine) -> Result<(TestUi, FocusHandle), TestError> {
    let (mut ui, focus) = component_test().size(800.0, 600.0).setup(engine.install()).build_with(|| {
        let focus = focus_handle();
        let view = view! {
            <div size_full flex col items_start>
                <web_card page={pricing()} name="pricing.html" track_focus=focus w=480.0 test_id="card"/>
                <div test_id="elsewhere" w=480.0 h=100.0/>
            </div>
        };
        (view, focus)
    });
    ui.wait_for_idle()?;
    Ok((ui, focus))
}

fn stage<T: View>(engine: &FakeEngine, root: impl FnOnce() -> T) -> Result<TestUi, TestError> {
    let mut ui = component_test()
        .size(800.0, 600.0)
        .setup(engine.install())
        .build(root);
    ui.wait_for_idle()?;
    Ok(ui)
}
