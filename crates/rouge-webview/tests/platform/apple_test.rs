#![allow(unsafe_code)]

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    macos::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
mod documents;

#[cfg(target_os = "macos")]
mod harness;

#[cfg(target_os = "macos")]
mod macos {
    use super::documents::{ANNOUNCES, AWKWARD, ECHOES, echo};
    use super::harness;
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSView};
    use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSRunLoop};
    use raw_window_handle::{
        AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle,
    };
    use rouge_core::Rect;
    use rouge_webview::{
        EnginePage, PageCommand, Placement, SystemEngine, WebContent, WebEngine, WebViewEvent,
        WebViewEvents, WebViewOptions,
    };
    use std::error::Error;
    use std::ptr::NonNull;
    use std::sync::mpsc::{Receiver, channel};

    type TestResult = Result<(), Box<dyn Error>>;
    type Test = fn(MainThreadMarker) -> TestResult;

    const MEASURES: &str = "<script>const size = () => innerWidth + ' by ' + innerHeight;\
                            addEventListener('resize', () => parent.postMessage('resized to ' + size(), '*'));\
                            addEventListener('message', (event) => {\
                            if (!event.source) parent.postMessage(size(), '*');\
                            });parent.postMessage(size(), '*')</script>";
    const THROWS: &str = "<script>addEventListener('error', (event) => parent.postMessage(event.message, '*'))</script>\
                          <script>null.missing</script>";

    pub(super) fn run() -> TestResult {
        let mtm = MainThreadMarker::new().ok_or("the engine test runs on the main thread")?;
        let _app = NSApplication::sharedApplication(mtm);
        let tests: [(&str, Test); 4] = [
            (
                "an_inline_page_runs_again_when_reloaded",
                an_inline_page_runs_again_when_reloaded,
            ),
            (
                "an_inline_page_reads_its_own_script_errors",
                an_inline_page_reads_its_own_script_errors,
            ),
            (
                "a_posted_message_reaches_the_page_as_it_was_written",
                a_posted_message_reaches_the_page_as_it_was_written,
            ),
            (
                "a_page_scaled_at_one_layout_size_is_never_resized",
                a_page_scaled_at_one_layout_size_is_never_resized,
            ),
        ];
        for (name, test) in harness::selected(&tests) {
            test(mtm)?;
            println!("test {name} ... ok");
        }
        Ok(())
    }

    fn an_inline_page_runs_again_when_reloaded(mtm: MainThreadMarker) -> TestResult {
        let mut stage = Stage::open(mtm)?;
        stage
            .page
            .apply(PageCommand::Load(WebContent::html(ANNOUNCES)));
        assert_eq!(stage.message(), "loaded");
        stage.page.apply(PageCommand::Reload);
        assert_eq!(stage.message(), "loaded");
        Ok(())
    }

    fn an_inline_page_reads_its_own_script_errors(mtm: MainThreadMarker) -> TestResult {
        let mut stage = Stage::open(mtm)?;
        stage
            .page
            .apply(PageCommand::Load(WebContent::html(THROWS)));
        let message = stage.message();
        assert!(message.contains("null"), "the page saw {message:?}");
        Ok(())
    }

    fn a_posted_message_reaches_the_page_as_it_was_written(mtm: MainThreadMarker) -> TestResult {
        let mut stage = Stage::open(mtm)?;
        stage
            .page
            .apply(PageCommand::Load(WebContent::html(ECHOES)));
        assert_eq!(stage.message(), "ready");
        stage.page.apply(PageCommand::Post(AWKWARD.into()));
        assert_eq!(stage.message(), echo());
        Ok(())
    }

    fn a_page_scaled_at_one_layout_size_is_never_resized(mtm: MainThreadMarker) -> TestResult {
        let mut stage = Stage::open(mtm)?;
        let place = |stage: &mut Stage, width: f32, height: f32| {
            let frame = Rect::new(20.0, 30.0, width, 600.0 * width / 900.0);
            stage.page.apply(PageCommand::Place(Placement {
                frame,
                clip: Rect::new(20.0, 30.0, width, height),
                page_zoom: width / 900.0,
                device_scale: 2.0,
                interactive: true,
                ..Placement::default()
            }));
        };
        place(&mut stage, 300.0, 169.0);
        stage
            .page
            .apply(PageCommand::Load(WebContent::html(MEASURES)));
        assert_eq!(stage.message(), "900 by 600");
        for (width, height) in [(451.0, 277.0), (743.0, 512.0), (900.0, 600.0)] {
            place(&mut stage, width, height);
        }
        stage.page.apply(PageCommand::Post("measure".into()));
        assert_eq!(
            stage.message(),
            "900 by 600",
            "the page heard a resize, or changed size, while its frame grew"
        );
        place(&mut stage, 450.0, 300.0);
        stage.page.apply(PageCommand::Place(Placement {
            frame: Rect::new(0.0, 0.0, 450.0, 300.0),
            clip: Rect::new(0.0, 0.0, 450.0, 300.0),
            page_zoom: 1.0,
            device_scale: 2.0,
            interactive: true,
            ..Placement::default()
        }));
        assert_eq!(stage.message(), "resized to 450 by 300");
        Ok(())
    }

    struct Stage {
        page: Box<dyn EnginePage>,
        events: Receiver<WebViewEvent>,
    }

    impl Stage {
        fn open(mtm: MainThreadMarker) -> Result<Self, Box<dyn Error>> {
            let (sender, events) = channel();
            let engine = SystemEngine(Host(NSView::new(mtm)));
            let page = engine.open(
                WebViewOptions::default(),
                WebViewEvents::new(move |event| {
                    let _ = sender.send(event);
                }),
            )?;
            Ok(Stage { page, events })
        }

        fn message(&self) -> String {
            loop {
                while let Ok(event) = self.events.try_recv() {
                    if let WebViewEvent::Message(text) = event {
                        return text;
                    }
                }
                NSRunLoop::currentRunLoop().runMode_beforeDate(
                    // SAFETY: a constant Foundation exports.
                    unsafe { NSDefaultRunLoopMode },
                    &NSDate::dateWithTimeIntervalSinceNow(0.01),
                );
            }
        }
    }

    struct Host(Retained<NSView>);

    impl HasWindowHandle for Host {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let view = NonNull::from(&*self.0).cast();
            // SAFETY: the view is retained by `self`, so it outlives the borrowed handle.
            Ok(unsafe {
                WindowHandle::borrow_raw(RawWindowHandle::AppKit(AppKitWindowHandle::new(view)))
            })
        }
    }
}
