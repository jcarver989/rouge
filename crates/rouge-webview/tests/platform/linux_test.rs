#![allow(unsafe_code)]

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    linux::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod linux {
    use gdkx11::X11Window;
    use glib::object::Cast;
    use gtk::prelude::{GtkWindowExt, WidgetExt};
    use raw_window_handle::{
        HandleError, HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle,
    };
    use rouge_core::testing::{TestUi, TestUiBuilder};
    use rouge_core::{Styled, div, view};
    use rouge_reactive::{Runtime, Scope, Signal, signal};
    use rouge_webview::{SystemEngine, WebContent, provide_web_engine, web_page, web_view};
    use std::env::args;
    use std::error::Error;
    use std::os::raw::c_ulong;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    type TestResult = Result<(), Box<dyn Error>>;
    type Test = fn() -> TestResult;

    const ANNOUNCES: &str = "<script>parent.postMessage('loaded', '*')</script>";
    const ECHOES: &str = "<script>addEventListener('message', (event) => {\
                          if (!event.source) parent.postMessage('echo ' + event.data, '*');\
                          });parent.postMessage('ready', '*')</script>";
    const AWKWARD: &str = "she said \"</script>\" \\ and\nleft\u{2028}";
    const PATIENCE: Duration = Duration::from_secs(30);

    pub(super) fn run() -> TestResult {
        let tests: [(&str, Test); 2] = [
            (
                "pages_showing_the_same_html_each_reload_their_own_document",
                pages_showing_the_same_html_each_reload_their_own_document,
            ),
            (
                "a_posted_message_reaches_the_page_as_it_was_written",
                a_posted_message_reaches_the_page_as_it_was_written,
            ),
        ];
        let filter = args().nth(1).filter(|arg| !arg.starts_with('-'));
        for (name, test) in tests {
            if filter
                .as_deref()
                .is_some_and(|filter| !name.contains(filter))
            {
                continue;
            }
            test()?;
            println!("test {name} ... ok");
        }
        Ok(())
    }

    fn pages_showing_the_same_html_each_reload_their_own_document() -> TestResult {
        let _rt = Runtime::new().enter();
        let host = Host::open()?;
        let (kept, closed): (Signal<Vec<String>>, Signal<Vec<String>>) =
            (signal(Vec::new()), signal(Vec::new()));
        let (mut ui, (page, twin)) = TestUiBuilder::default().size(800.0, 600.0).build_with(|| {
            provide_web_engine(SystemEngine(host));
            let page = web_page(WebContent::html(ANNOUNCES))
                .on_message(move |message: String| kept.update(|heard| heard.push(message)));
            let twin = Scope::new();
            let other = twin.run(|| {
                web_page(WebContent::html(ANNOUNCES))
                    .on_message(move |message: String| closed.update(|heard| heard.push(message)))
            });
            let view = view! {
                <div>
                    <web_view page w=200.0 h=100.0/>
                    <web_view page=other w=200.0 h=100.0/>
                </div>
            };
            (view, (page, twin))
        });
        until(&mut ui, || {
            kept.with(Vec::len) == 1 && closed.with(Vec::len) == 1
        })?;
        twin.dispose();
        page.reload();
        until(&mut ui, || kept.with(Vec::len) == 2)?;
        Ok(())
    }

    fn a_posted_message_reaches_the_page_as_it_was_written() -> TestResult {
        let _rt = Runtime::new().enter();
        let host = Host::open()?;
        let heard: Signal<Vec<String>> = signal(Vec::new());
        let (mut ui, page) = TestUiBuilder::default().size(800.0, 600.0).build_with(|| {
            provide_web_engine(SystemEngine(host));
            let page = web_page(WebContent::html(ECHOES))
                .on_message(move |message: String| heard.update(|heard| heard.push(message)));
            (view! { <web_view page w=200.0 h=100.0/> }, page)
        });
        until(&mut ui, || heard.with(Vec::len) == 1)?;
        page.post_message(AWKWARD);
        until(&mut ui, || heard.with(Vec::len) == 2)?;
        assert_eq!(
            heard.get(),
            vec!["ready".to_owned(), format!("echo {AWKWARD}")]
        );
        Ok(())
    }

    fn until(ui: &mut TestUi, done: impl Fn() -> bool) -> TestResult {
        let deadline = Instant::now() + PATIENCE;
        while !done() {
            if Instant::now() > deadline {
                return Err("the page did not answer in time".into());
            }
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
            ui.run(0.02);
            sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    struct Host {
        _window: gtk::Window,
        xid: c_ulong,
    }

    impl Host {
        fn open() -> Result<Self, Box<dyn Error>> {
            gtk::init()?;
            let window = gtk::Window::new(gtk::WindowType::Toplevel);
            window.set_default_size(800, 600);
            window.show();
            let xid = window
                .window()
                .and_then(|window| window.downcast::<X11Window>().ok())
                .ok_or("the test needs an X11 display")?
                .xid();
            Ok(Host {
                _window: window,
                xid,
            })
        }
    }

    impl HasWindowHandle for Host {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let handle = RawWindowHandle::Xlib(XlibWindowHandle::new(self.xid));
            // SAFETY: the GTK window that owns this X window is kept by `self`.
            Ok(unsafe { WindowHandle::borrow_raw(handle) })
        }
    }
}
