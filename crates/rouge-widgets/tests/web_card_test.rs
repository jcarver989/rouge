use rouge_core::testing::{TestResult, TestUi};
use rouge_core::{Corners, FocusHandle, InputKind, Styled, View, div, focus_handle, view};
use rouge_reactive::{Runtime, signal};
use rouge_webview::testing::FakeEngine;
use rouge_webview::{
    PageCommand, WebContent, WebPage, WebViewError, WebViewEvent, provide_web_engine, web_page,
};
use rouge_widgets::testing::component_test;
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
    });
    ui.run(0.2);
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
    let (mut ui, focus) = card(&engine);
    ui.run(0.2);
    assert!(!engine.placement(0).ok_or("asleep")?.interactive);
    assert!(ui.text_bounds("Click to interact").is_some());
    ui.click_on("card-viewport")?;
    ui.run(0.2);
    assert!(focus.is_focused());
    assert!(engine.placement(0).ok_or("awake")?.interactive);
    assert!(ui.text_bounds("Click outside to return").is_some());
    ui.click_on("elsewhere")?;
    ui.run(0.2);
    assert!(!focus.is_focused());
    assert!(!engine.placement(0).ok_or("asleep again")?.interactive);
    Ok(())
}

#[test]
fn escape_and_the_owner_can_both_put_an_awake_card_to_sleep() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let (mut ui, focus) = card(&engine);
    ui.run(0.2);
    focus.focus();
    ui.run(0.2);
    assert!(engine.placement(0).ok_or("woken by its owner")?.interactive);
    ui.press("escape");
    ui.run(0.2);
    assert!(!focus.is_focused(), "Escape returns to the thread");
    focus.focus();
    ui.run(0.2);
    assert!(engine.placement(0).ok_or("woken")?.interactive);
    focus.blur();
    ui.run(0.2);
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
    });
    ui.run(0.2);
    ui.click_on("card-reload")?;
    ui.click_on("card-open")?;
    ui.run(0.2);
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
    });
    ui.run(0.2);
    assert!(ui.text_bounds("Click to open").is_some());
    ui.click_on("card-viewport")?;
    ui.run(0.2);
    assert_eq!(expanded.get(), 1);
    assert!(
        !engine.placement(0).ok_or("placed")?.interactive,
        "the page woke in place"
    );
    ui.click_on("card-expand")?;
    ui.run(0.2);
    assert_eq!(expanded.get(), 2);
    Ok(())
}

#[test]
fn an_expandable_card_given_a_focus_handle_still_expands() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let expanded = signal(0);
    let (mut ui, focus) = component_test().size(800.0, 600.0).build_with({
        let engine = engine.clone();
        move || {
            provide_web_engine(engine);
            let focus = focus_handle();
            let view = view! {
                <web_card page={pricing()} name="pricing.html"
                     on_expand={move || expanded.update(|count| *count += 1)} track_focus=focus w=480.0
                     test_id="card"/>
            };
            (view, focus)
        }
    });
    ui.run(0.2);
    ui.click_on("card-viewport")?;
    focus.focus();
    ui.run(0.2);
    assert_eq!(expanded.get(), 1);
    assert!(!focus.is_focused(), "an expanding card took focus");
    assert!(!engine.placement(0).ok_or("placed")?.interactive);
    Ok(())
}

#[test]
fn a_card_with_nowhere_to_open_has_no_open_button() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let (mut ui, _focus) = card(&engine);
    ui.run(0.2);
    assert!(ui.bounds_of("card-reload").is_some());
    assert_eq!(ui.bounds_of("card-open"), None);
    assert_eq!(ui.bounds_of("card-expand"), None);
    Ok(())
}

#[test]
fn the_header_says_what_the_page_is_doing() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let (mut ui, _) = card(&engine);
    ui.run(0.2);
    engine.emit(0, WebViewEvent::Loading(true));
    ui.run(0.2);
    assert_eq!(ui.text_of("card-status").as_deref(), Some("Loading"));
    engine.emit(0, WebViewEvent::Failed("The server is down.".into()));
    ui.run(0.2);
    assert_eq!(
        ui.text_of("card-status").as_deref(),
        Some("Couldn't load: The server is down.")
    );
    engine.emit(0, WebViewEvent::Loading(true));
    engine.emit(0, WebViewEvent::Loading(false));
    ui.run(0.2);
    assert_eq!(
        ui.text_of("card-status").as_deref(),
        Some("Click to interact")
    );
    Ok(())
}

#[test]
fn messages_from_the_page_reach_the_cards_owner() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new();
    let heard = signal(String::new());
    let mut ui = stage(&engine, move || {
        let page = pricing().on_message(move |message: String| heard.set(message));
        view! { <web_card page name="pricing.html" w=480.0/> }
    });
    ui.run(0.2);
    engine.emit(0, WebViewEvent::Message("saved".into()));
    ui.run(0.2);
    assert_eq!(heard.get(), "saved");
    Ok(())
}

#[test]
fn a_platform_without_web_views_gets_a_card_that_says_so() -> TestResult {
    let _entered = Runtime::new().enter();
    let engine = FakeEngine::new().failing(WebViewError::Unsupported);
    let (mut ui, focus) = card(&engine);
    ui.run(0.2);
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
        .build({
            let engine = engine.clone();
            move || {
                provide_web_engine(engine);
                view! { <web_card page={pricing()} name="pricing.html" test_id="card"/> }
            }
        });
    ui.run(0.2);
    assert_eq!(
        ui.text_of("card-status").as_deref(),
        Some("Tap to interact")
    );
    ui.tap_on("card-viewport")?;
    ui.run(0.2);
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

fn card(engine: &FakeEngine) -> (TestUi, FocusHandle) {
    component_test().size(800.0, 600.0).build_with(move || {
        provide_web_engine(engine.clone());
        let focus = focus_handle();
        let view = view! {
            <div size_full flex col items_start>
                <web_card page={pricing()} name="pricing.html" track_focus=focus w=480.0 test_id="card"/>
                <div test_id="elsewhere" w=480.0 h=100.0/>
            </div>
        };
        (view, focus)
    })
}

fn stage<T: View>(engine: &FakeEngine, root: impl FnOnce() -> T) -> TestUi {
    component_test().size(800.0, 600.0).build(move || {
        provide_web_engine(engine.clone());
        root()
    })
}
