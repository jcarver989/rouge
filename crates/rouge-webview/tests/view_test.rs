use rouge_core::testing::{TestError, TestResult, TestUi, TestUiBuilder};
use rouge_core::{
    Corners, Declaration, Point, Prop, Rect, Size, Styled, View, div, occlusions, scroll, show,
    text, view, zoom,
};
use rouge_reactive::{Runtime, Scope, Signal, memo, signal};
use rouge_webview::testing::FakeEngine;
use rouge_webview::{
    PageCommand, PageState, Placement, WebContent, WebPage, WebViewError, WebViewEvent,
    WebViewOptions, web_page, web_view,
};

const SITE: &str = "https://example.com/";

#[test]
fn a_web_view_is_placed_where_layout_puts_it_and_shows_its_content() -> TestResult {
    let _rt = Runtime::new().enter();
    let (engine, mut ui) = stage(|| {
        let page = WebViewOptions { inspectable: true }.open(WebContent::url(SITE));
        view! {
            <div p=20.0>
                <web_view page w=300.0 h=200.0 rounded={Corners::all(8.0)} test_id="page"
                     hover:rounded={Corners::all(16.0)}/>
            </div>
        }
    })?;
    assert_eq!(engine.opened(), 1);
    assert_eq!(engine.content(0), Some(WebContent::url(SITE)));
    let placed = placed(&engine)?;
    assert_eq!(placed.frame, Rect::new(20.0, 20.0, 300.0, 200.0));
    assert_eq!(placed.clip, placed.frame);
    assert_eq!(placed.corners, Corners::all(8.0));
    assert!(placed.interactive && placed.visible());
    assert_eq!(placed.page_zoom, 1.0);
    assert!(engine.options(0).ok_or("options")?.inspectable);
    ui.hover_on("page")?;
    ui.wait_for_idle()?;
    assert_eq!(self::placed(&engine)?.corners, Corners::all(16.0));
    Ok(())
}

#[test]
fn a_settled_web_view_is_not_placed_or_loaded_again() -> TestResult {
    let _rt = Runtime::new().enter();
    let label = signal("one".to_owned());
    let (engine, mut ui) = stage(move || {
        view! {
            <div>
                <web_view page={site()} w=300.0 h=200.0/>
                <text>{move || label.get()}</text>
            </div>
        }
    })?;
    engine.take_calls(0);
    label.set("two".into());
    ui.wait_for_idle()?;
    assert_eq!(engine.calls(0), Vec::new(), "an unrelated repaint");
    Ok(())
}

#[test]
fn new_content_is_loaded_into_the_same_page_once() -> TestResult {
    let _rt = Runtime::new().enter();
    let (engine, _ui, page) = stage_with(|| {
        let page = web_page(WebContent::html("<h1>v1</h1>"));
        (view! { <web_view page w=300.0 h=200.0/> }, page)
    })?;
    engine.take_calls(0);
    page.load(WebContent::html("<h1>v2</h1>"));
    page.load(WebContent::html("<h1>v2</h1>"));
    assert_eq!(engine.opened(), 1);
    assert_eq!(
        engine.calls(0),
        vec![PageCommand::Load(WebContent::html("<h1>v2</h1>"))]
    );
    Ok(())
}

#[test]
fn a_web_view_follows_its_scroller_and_is_clipped_to_it() -> TestResult {
    let _rt = Runtime::new().enter();
    let (engine, mut ui) = stage(|| {
        view! {
            <scroll test_id="scroller" w=300.0 h=200.0>
                <div flex col>
                    <div h=100.0/>
                    <web_view page={site()} w=300.0 h=150.0 flex_none rounded={Corners::all(10.0)}/>
                    <div h=600.0 flex_none/>
                </div>
            </scroll>
        }
    })?;
    let resting = placed(&engine)?;
    assert_eq!(resting.frame, Rect::new(0.0, 100.0, 300.0, 150.0));
    assert_eq!(resting.clip, Rect::new(0.0, 100.0, 300.0, 100.0));
    let cut = resting.layout(1.0).ok_or("a visible page has a layout")?;
    assert_eq!(cut.corners.bottom_left, 0.0);
    ui.scroll_on("scroller", Point::new(0.0, -160.0))?;
    ui.wait_for_idle()?;
    let scrolled = placed(&engine)?;
    assert_eq!(scrolled.frame, Rect::new(0.0, -60.0, 300.0, 150.0));
    assert_eq!(scrolled.clip, Rect::new(0.0, 0.0, 300.0, 90.0));
    ui.scroll_on("scroller", Point::new(0.0, -400.0))?;
    ui.wait_for_idle()?;
    let gone = placed(&engine)?;
    assert!(!gone.visible(), "scrolled out of its scroller: {gone:?}");
    Ok(())
}

#[test]
fn hiding_a_web_view_hides_its_page() -> TestResult {
    let _rt = Runtime::new().enter();
    let hidden = signal(false);
    let (engine, mut ui) = stage(move || {
        view! {
            <div size_full declare={Prop::Dynamic(Box::new(move || Declaration::Invisible(hidden.get())))}>
                <web_view page={site()} w=300.0 h=200.0/>
            </div>
        }
    })?;
    assert!(placed(&engine)?.visible());
    hidden.set(true);
    ui.wait_for_idle()?;
    assert!(!placed(&engine)?.visible());
    assert!(!engine.closed(0));
    Ok(())
}

#[test]
fn a_page_made_with_its_web_view_closes_with_it() -> TestResult {
    let _rt = Runtime::new().enter();
    let mounted = signal(true);
    let (engine, mut ui) = stage(move || {
        view! {
            <div size_full>
                <show when={move || mounted.get()}>
                    <web_view page={site()} w=300.0 h=200.0/>
                </show>
            </div>
        }
    })?;
    mounted.set(false);
    ui.wait_for_idle()?;
    assert!(engine.closed(0), "the native page outlived its element");
    mounted.set(true);
    ui.wait_for_idle()?;
    assert_eq!(engine.opened(), 2);
    Ok(())
}

#[test]
fn a_page_outlives_the_web_views_that_show_it() -> TestResult {
    let _rt = Runtime::new().enter();
    let mounted = signal(true);
    let heard = signal(String::new());
    let (engine, mut ui, (owner, page, covered)) = stage_with(move || {
        let owner = Scope::new();
        let page = owner.run(site);
        page.on_message(move |message: String| heard.set(message));
        let view = view! {
            <div size_full>
                <show when={move || mounted.get()}>
                    <web_view page w=300.0 h=200.0/>
                </show>
            </div>
        };
        (view, (owner, page, memo(occlusions)))
    })?;
    mounted.set(false);
    ui.wait_for_idle()?;
    assert!(!engine.closed(0), "the page closed with its element");
    assert!(
        !placed(&engine)?.visible(),
        "a page with no element is hidden"
    );
    page.reload();
    engine.emit(0, WebViewEvent::Message("still here".into()));
    mounted.set(true);
    ui.wait_for_idle()?;
    assert_eq!(engine.opened(), 1);
    assert!(placed(&engine)?.visible());
    assert!(engine.calls(0).contains(&PageCommand::Reload));
    assert_eq!(heard.get(), "still here");
    owner.dispose();
    ui.wait_for_idle()?;
    assert!(engine.closed(0), "the page outlived its scope");
    assert!(
        covered.get().is_empty(),
        "the dead page still occludes its mounted view"
    );
    Ok(())
}

#[test]
fn the_newest_web_view_given_a_page_shows_it() -> TestResult {
    let _rt = Runtime::new().enter();
    let inline = signal(true);
    let (engine, mut ui) = stage(move || {
        let page = web_page(WebContent::html("<p>made</p>"));
        view! {
            <div size_full>
                <show when={move || inline.get()}>
                    <web_view page w=300.0 h=200.0/>
                </show>
                <show when={move || !inline.get()}>
                    <web_view page w=700.0 h=500.0/>
                </show>
            </div>
        }
    })?;
    assert_eq!(placed(&engine)?.frame.size, Size::new(300.0, 200.0));
    inline.set(false);
    ui.wait_for_idle()?;
    assert_eq!(engine.opened(), 1);
    let loads = engine
        .calls(0)
        .into_iter()
        .filter(|call| matches!(call, PageCommand::Load(_)))
        .count();
    assert_eq!(loads, 1, "moving the page loaded it again");
    assert_eq!(placed(&engine)?.frame.size, Size::new(700.0, 500.0));
    Ok(())
}

#[test]
fn a_page_returns_to_the_web_view_that_stayed_mounted_beneath_a_newer_one() -> TestResult {
    let _rt = Runtime::new().enter();
    let expanded = signal(false);
    let wide = signal(false);
    let (engine, mut ui, covered) = stage_with(move || {
        let page = web_page(WebContent::html("<p>made</p>"));
        let view = view! {
            <div size_full>
                <web_view page w={move || if wide.get() { 400.0 } else { 300.0 }} h=200.0/>
                <show when={move || expanded.get()}>
                    <web_view page w=700.0 h=500.0/>
                </show>
            </div>
        };
        (view, memo(occlusions))
    })?;
    assert_eq!(placed(&engine)?.frame.size, Size::new(300.0, 200.0));
    expanded.set(true);
    ui.wait_for_idle()?;
    assert_eq!(placed(&engine)?.frame.size, Size::new(700.0, 500.0));
    assert_eq!(covered.get(), [placed(&engine)?.clip]);
    let count = engine.calls(0).len();
    wide.set(true);
    ui.wait_for_idle()?;
    assert_eq!(
        placed(&engine)?.frame.size,
        Size::new(700.0, 500.0),
        "the covered web view moved the page"
    );
    assert_eq!(covered.get(), [placed(&engine)?.clip]);
    assert_eq!(
        engine.calls(0).len(),
        count,
        "a losing mount issued a command"
    );
    expanded.set(false);
    ui.wait_for_idle()?;
    assert_eq!(
        placed(&engine)?.frame.size,
        Size::new(400.0, 200.0),
        "the page did not return to where the card is now"
    );
    assert_eq!(covered.get(), [placed(&engine)?.clip]);
    assert_eq!(engine.opened(), 1);
    Ok(())
}

#[test]
fn an_exclusive_web_view_hides_the_other_pages_in_its_window() -> TestResult {
    let _rt = Runtime::new().enter();
    let covering = signal(false);
    let nested = signal(false);
    let (other_window, _other_ui) = stage(|| view! { <web_view page={site()} size_full/> })?;
    let (engine, mut ui, covered) = stage_with(move || {
        let view = view! {
            <div size_full>
                <web_view page={site()} w=300.0 h=200.0/>
                <show when={move || covering.get()}>
                    <web_view page={site()} exclusive absolute inset=0.0/>
                </show>
                <show when={move || nested.get()}>
                    <web_view page={site()} exclusive absolute w=500.0 h=400.0/>
                </show>
            </div>
        };
        (view, memo(occlusions))
    })?;
    assert!(placed(&engine)?.visible());
    covering.set(true);
    ui.wait_for_idle()?;
    assert!(!placed(&engine)?.visible(), "the covered page still shows");
    let front = engine.placement(1).ok_or("the exclusive page")?;
    assert_eq!(front.frame, Rect::new(0.0, 0.0, 800.0, 600.0));
    nested.set(true);
    ui.wait_for_idle()?;
    let top = engine.placement(2).ok_or("nested page")?;
    assert!(top.visible());
    assert!(!engine.placement(1).ok_or("covered exclusive")?.visible());
    assert_eq!(covered.get(), [top.clip]);
    assert!(placed(&other_window)?.visible());
    nested.set(false);
    ui.wait_for_idle()?;
    assert_eq!(engine.placement(1), Some(front));
    assert_eq!(covered.get(), [front.clip]);
    covering.set(false);
    ui.wait_for_idle()?;
    assert!(placed(&engine)?.visible());
    Ok(())
}

#[test]
fn window_zoom_scales_the_frame_and_the_page_together() -> TestResult {
    let _rt = Runtime::new().enter();
    let engine = FakeEngine::new();
    let (mut ui, (window_zoom, covered)) = TestUiBuilder::default()
        .scale(2.0)
        .setup(engine.install())
        .build_with(|| {
            let view = view! {
                <div p=10.0>
                    <web_view page={site()} w=200.0 h=100.0 page_zoom=0.5/>
                </div>
            };
            (view, (zoom(), memo(occlusions)))
        });
    ui.wait_for_idle()?;
    let unzoomed = placed(&engine)?;
    assert_eq!(unzoomed.device_scale, 2.0);
    assert_eq!(unzoomed.frame, Rect::new(10.0, 10.0, 200.0, 100.0));
    assert_eq!(unzoomed.page_zoom, 0.5);
    window_zoom.set(2.0);
    ui.wait_for_idle()?;
    let zoomed = placed(&engine)?;
    assert_eq!(zoomed.frame, Rect::new(20.0, 20.0, 400.0, 200.0));
    assert_eq!(zoomed.page_zoom, 1.0);
    assert_eq!(
        covered.get(),
        [zoomed.clip.scale(0.5)],
        "layout coordinates"
    );
    Ok(())
}

#[test]
fn a_page_that_is_not_interactive_is_placed_that_way() -> TestResult {
    let _rt = Runtime::new().enter();
    let awake = signal(false);
    let (engine, mut ui) = stage(move || {
        view! { <web_view page={site()} interactive={move || awake.get()} w=300.0 h=200.0/> }
    })?;
    assert!(!placed(&engine)?.interactive);
    awake.set(true);
    ui.wait_for_idle()?;
    assert!(placed(&engine)?.interactive);
    Ok(())
}

#[test]
fn the_page_reports_what_it_does() -> TestResult {
    let _rt = Runtime::new().enter();
    let (engine, mut ui, page) = staged_site()?;
    assert_eq!(page.error(), None);
    engine.emit(0, WebViewEvent::Loading(true));
    engine.emit(0, WebViewEvent::Progress(0.4));
    engine.emit(0, WebViewEvent::Url(SITE.into()));
    engine.emit(0, WebViewEvent::Title("Example Domain".into()));
    engine.emit(
        0,
        WebViewEvent::History {
            back: true,
            forward: false,
        },
    );
    ui.wait_for_idle()?;
    let loading = PageState {
        url: SITE.into(),
        title: "Example Domain".into(),
        loading: true,
        progress: 0.4,
        can_go_back: true,
        can_go_forward: false,
        failure: None,
    };
    assert_eq!(page.state().get(), loading);
    engine.emit(0, WebViewEvent::Failed("offline".into()));
    ui.wait_for_idle()?;
    let failed = PageState {
        loading: false,
        failure: Some("offline".into()),
        ..loading
    };
    assert_eq!(page.state().get(), failed, "a failed load has stopped");
    engine.emit(0, WebViewEvent::Loading(true));
    ui.wait_for_idle()?;
    assert_eq!(page.state().get().failure, None, "a new load starts clean");
    Ok(())
}

#[test]
fn commands_reach_the_page_in_order_even_before_a_web_view_shows_it() -> TestResult {
    let _rt = Runtime::new().enter();
    let (engine, _ui) = stage(|| {
        let page = site();
        page.post_message("early");
        page.reload();
        page.go_back();
        page.go_forward();
        page.stop();
        view! { <web_view page/> }
    })?;
    let commands: Vec<PageCommand> = engine
        .calls(0)
        .into_iter()
        .filter(|call| !matches!(call, PageCommand::Load(_) | PageCommand::Place(_)))
        .collect();
    assert_eq!(
        commands,
        vec![
            PageCommand::Post("early".into()),
            PageCommand::Reload,
            PageCommand::Back,
            PageCommand::Forward,
            PageCommand::Stop,
        ]
    );
    Ok(())
}

#[test]
fn messages_from_the_page_reach_its_listener() -> TestResult {
    let _rt = Runtime::new().enter();
    let heard: Signal<Vec<String>> = signal(Vec::new());
    let (engine, mut ui, page) = staged_site()?;
    page.on_message(move |message: String| heard.update(|heard| heard.push(message)));
    engine.emit(0, WebViewEvent::Message("ready".into()));
    engine.emit(0, WebViewEvent::Message("{\"height\":420}".into()));
    ui.wait_for_idle()?;
    assert_eq!(heard.get(), vec!["ready", "{\"height\":420}"]);
    Ok(())
}

#[test]
fn a_window_without_a_web_view_shows_the_fallback() -> TestResult {
    let _rt = Runtime::new().enter();
    let engine = FakeEngine::new().failing(WebViewError::Unsupported);
    let (mut ui, page) = TestUiBuilder::default().setup(engine.install()).build_with(|| {
        let page = site();
        let view = view! { <web_view page fallback={|error| view! { <text>{error.to_string()}</text> }}/> };
        (view, page)
    });
    ui.wait_for_idle()?;
    assert_eq!(engine.opened(), 0);
    assert_eq!(page.error(), Some(WebViewError::Unsupported));
    assert!(
        ui.text_bounds("no system web view is available on this platform")
            .is_some()
    );
    Ok(())
}

#[test]
fn a_headless_window_has_no_system_web_view() -> TestResult {
    let _rt = Runtime::new().enter();
    let (_ui, page) = TestUiBuilder::default().build_with(|| {
        let page = site();
        (view! { <web_view page/> }, page)
    });
    assert_eq!(page.error(), Some(WebViewError::NoWindow));
    Ok(())
}

fn site() -> WebPage {
    web_page(WebContent::url(SITE))
}

fn staged_site() -> Result<(FakeEngine, TestUi, WebPage), TestError> {
    stage_with(|| {
        let page = site();
        (view! { <web_view page w=300.0 h=200.0/> }, page)
    })
}

fn stage<T: View>(root: impl FnOnce() -> T) -> Result<(FakeEngine, TestUi), TestError> {
    let (engine, ui, ()) = stage_with(|| (root(), ()))?;
    Ok((engine, ui))
}

fn stage_with<T: View, U>(
    root: impl FnOnce() -> (T, U),
) -> Result<(FakeEngine, TestUi, U), TestError> {
    let engine = FakeEngine::new();
    let (mut ui, made) = TestUiBuilder::default()
        .size(800.0, 600.0)
        .setup(engine.install())
        .build_with(root);
    ui.wait_for_idle()?;
    Ok((engine, ui, made))
}

fn placed(engine: &FakeEngine) -> Result<Placement, &'static str> {
    engine.placement(0).ok_or("the page was never placed")
}
