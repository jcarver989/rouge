use super::SystemEngine;
use super::shared::inline::{Inline, SCHEME};
use super::shared::message_handler::{BRIDGE_NAME, BRIDGE_SCRIPT};
use crate::content::WebContent;
use crate::engine::{
    EnginePage, PageCommand, Placement, WebEngine, WebViewEvent, WebViewEvents, WebViewOptions,
};
use crate::error::WebViewError;
use gdkx11::{X11Display, X11Window};
use glib::object::Cast;
use glib::{Bytes, VariantDict};
use gtk::cairo::{RectangleInt, Region};
use gtk::gio::{Cancellable, IOErrorEnum, MemoryInputStream};
use gtk::prelude::{ContainerExt, FixedExt, GtkWindowExt, WidgetExt};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use rouge_core::{Corners, window_scope};
use rouge_reactive::{Task, provide_context, sleep, spawn, use_context};
use std::cell::RefCell;
use std::os::raw::c_ulong;
use std::rc::{Rc, Weak};
use std::time::Duration;
use webkit2gtk::{
    LoadEvent, SettingsExt, URISchemeRequest, URISchemeRequestExt, UserContentInjectedFrames,
    UserContentManager, UserContentManagerExt, UserScript, UserScriptInjectionTime, WebContext,
    WebContextExt, WebView, WebViewExt,
};

const UNKNOWN_LENGTH: i64 = -1;
const PUMP_INTERVAL: Duration = Duration::from_millis(8);
const WAYLAND: &str =
    "web views need the X11 backend; unset WAYLAND_DISPLAY to run through XWayland";

#[derive(Clone)]
struct GtkPump(Weak<Task>);

impl<T: HasWindowHandle + 'static> WebEngine for SystemEngine<T> {
    fn open(
        &self,
        options: WebViewOptions,
        events: WebViewEvents,
    ) -> Result<Box<dyn EnginePage>, WebViewError> {
        let handle = self.0.window_handle().map_err(|_| WebViewError::NoWindow)?;
        let parent = match handle.as_raw() {
            RawWindowHandle::Xlib(handle) => handle.window,
            RawWindowHandle::Xcb(handle) => c_ulong::from(handle.window.get()),
            RawWindowHandle::Wayland(_) => {
                return Err(WebViewError::Create(WAYLAND.into()));
            }
            _ => return Err(WebViewError::NoWindow),
        };
        Page::open(parent, options, &events).map(|page| Box::new(page) as Box<dyn EnginePage>)
    }
}

fn failed(what: &str) -> WebViewError {
    WebViewError::Create(what.to_owned())
}

type Shown = Rc<RefCell<Option<Inline>>>;

fn context(shown: &Shown) -> WebContext {
    let context = WebContext::default()
        .and_then(|shared| shared.website_data_manager())
        .map_or_else(WebContext::new, |data| {
            WebContext::with_website_data_manager(&data)
        });
    let shown = shown.clone();
    context.register_uri_scheme(SCHEME, move |request| {
        serve(request, shown.borrow().as_ref());
    });
    context
}

fn serve(request: &URISchemeRequest, shown: Option<&Inline>) {
    let wanted = request.uri();
    match shown.filter(|inline| wanted.is_some_and(|wanted| wanted == inline.address)) {
        Some(inline) => {
            let length = i64::try_from(inline.document.len()).unwrap_or(UNKNOWN_LENGTH);
            let bytes = Bytes::from(inline.document.as_bytes());
            request.finish(
                &MemoryInputStream::from_bytes(&bytes),
                length,
                Some("text/html"),
            );
        }
        None => request.finish_error(&mut glib::Error::new(
            IOErrorEnum::NotFound,
            "this page has no inline document at that address",
        )),
    }
}

fn pump() -> Rc<Task> {
    window_scope().run(|| {
        if let Some(task) = use_context::<GtkPump>().and_then(|pump| pump.0.upgrade()) {
            return task;
        }
        let task = Rc::new(
            spawn(async {
                loop {
                    while gtk::events_pending() {
                        gtk::main_iteration_do(false);
                    }
                    sleep(PUMP_INTERVAL).await;
                }
            })
            .cancel_on_drop(),
        );
        provide_context(GtkPump(Rc::downgrade(&task)));
        task
    })
}

struct Page {
    _pump: Rc<Task>,
    window: gtk::Window,
    surface: gdk::Window,
    fixed: gtk::Fixed,
    web: WebView,
    shown: Shown,
    visible: bool,
}

impl Page {
    fn open(
        parent: c_ulong,
        options: WebViewOptions,
        events: &WebViewEvents,
    ) -> Result<Self, WebViewError> {
        if !gtk::is_initialized() {
            gtk::init().map_err(|error| WebViewError::Create(error.to_string()))?;
        }
        let display = gdk::Display::default()
            .and_then(|display| display.downcast::<X11Display>().ok())
            .ok_or_else(|| failed("GTK is not running on X11"))?;
        let host = X11Window::foreign_new_for_display(&display, parent);

        let manager = UserContentManager::new();
        manager.register_script_message_handler(BRIDGE_NAME);
        manager.add_script(&UserScript::new(
            BRIDGE_SCRIPT,
            UserContentInjectedFrames::TopFrame,
            UserScriptInjectionTime::Start,
            &[],
            &[],
        ));
        let messages = events.clone();
        manager.connect_script_message_received(Some(BRIDGE_NAME), move |_, result| {
            if let Some(value) = result.js_value() {
                messages.send(WebViewEvent::Message(value.to_string()));
            }
        });
        let shown = Shown::default();
        let web = WebView::builder()
            .web_context(&context(&shown))
            .user_content_manager(&manager)
            .build();
        if let (true, Some(settings)) = (options.inspectable, WebViewExt::settings(&web)) {
            settings.set_enable_developer_extras(true);
        }
        report(&web, events);

        let window = gtk::Window::new(gtk::WindowType::Popup);
        let fixed = gtk::Fixed::new();
        fixed.put(&web, 0, 0);
        window.add(&fixed);
        window.realize();
        let surface = window
            .window()
            .ok_or_else(|| failed("GTK did not create a window for the page"))?;
        surface.reparent(host.upcast_ref(), 0, 0);
        fixed.show_all();
        Ok(Page {
            _pump: pump(),
            window,
            surface,
            fixed,
            web,
            shown,
            visible: false,
        })
    }

    fn set_visible(&mut self, visible: bool) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if visible {
            self.window.show();
        } else {
            self.window.hide();
        }
    }
}

fn report(web: &WebView, events: &WebViewEvents) {
    let sink = events.clone();
    web.connect_load_changed(move |web, event| {
        match event {
            LoadEvent::Started => sink.send(WebViewEvent::Loading(true)),
            LoadEvent::Finished => sink.send(WebViewEvent::Loading(false)),
            _ => {}
        }
        sink.send(WebViewEvent::History {
            back: web.can_go_back(),
            forward: web.can_go_forward(),
        });
    });
    let sink = events.clone();
    web.connect_load_failed(move |_, _, _, error| {
        sink.send(WebViewEvent::Failed(error.to_string()));
        false
    });
    let sink = events.clone();
    web.connect_title_notify(move |web| {
        let title = web.title().map_or_default(|title| title.to_string());
        sink.send(WebViewEvent::Title(title));
    });
    let sink = events.clone();
    web.connect_uri_notify(move |web| {
        let url = web.uri().map_or_default(|url| url.to_string());
        sink.send(WebViewEvent::Url(url));
    });
    let sink = events.clone();
    web.connect_estimated_load_progress_notify(move |web| {
        sink.send(WebViewEvent::Progress(web.estimated_load_progress() as f32));
    });
}

fn rounded(width: i32, height: i32, corners: Corners) -> Region {
    let region = Region::create();
    let inset = |radius: f32, row: f32| {
        if radius <= 0.0 || row >= radius {
            return 0;
        }
        let rise = radius - row - 0.5;
        (radius - (radius * radius - rise * rise).max(0.0).sqrt()).round() as i32
    };
    for y in 0..height {
        let (above, below) = (y as f32, (height - 1 - y) as f32);
        let left = inset(corners.top_left, above).max(inset(corners.bottom_left, below));
        let right = inset(corners.top_right, above).max(inset(corners.bottom_right, below));
        let row = RectangleInt::new(left, y, (width - left - right).max(0), 1);
        let _ = region.union_rectangle(&row);
    }
    region
}

impl Page {
    fn load(&mut self, content: &WebContent) {
        match content {
            WebContent::Url(url) => self.web.load_uri(url),
            WebContent::Html { html, base_url } => {
                let inline = Inline::new(html, base_url.as_deref());
                let address = inline.address.clone();
                self.shown.replace(Some(inline));
                self.web.load_uri(&address);
            }
        }
    }

    fn place(&mut self, placement: &Placement) {
        let toolkit = self.window.scale_factor().max(1) as f32;
        let unit = placement.device_scale / toolkit;
        let layout = placement.layout(unit);
        self.set_visible(layout.is_some() && placement.visible());
        let Some(layout) = layout else {
            return;
        };
        let (clip, page) = (layout.clip, layout.page);
        let units = |length: f32| length.round() as i32;
        let (width, height) = (
            units(clip.size.width).max(1),
            units(clip.size.height).max(1),
        );
        self.window.resize(width, height);
        self.surface
            .move_resize(units(clip.origin.x), units(clip.origin.y), width, height);
        self.fixed
            .move_(&self.web, units(page.origin.x), units(page.origin.y));
        self.web.set_size_request(
            units(page.size.width).max(1),
            units(page.size.height).max(1),
        );
        self.web.set_zoom_level(f64::from(layout.zoom * unit));
        let shape = rounded(width, height, layout.corners);
        self.surface.shape_combine_region(Some(&shape), 0, 0);
        let hears = if layout.interactive {
            shape
        } else {
            Region::create()
        };
        self.surface.input_shape_combine_region(&hears, 0, 0);
    }
}

impl EnginePage for Page {
    fn apply(&mut self, command: PageCommand) {
        match command {
            PageCommand::Load(content) => self.load(&content),
            PageCommand::Place(placement) => self.place(&placement),
            PageCommand::Reload => self.web.reload(),
            PageCommand::Stop => self.web.stop_loading(),
            PageCommand::Back => self.web.go_back(),
            PageCommand::Forward => self.web.go_forward(),
            PageCommand::Post(message) => {
                let arguments = VariantDict::new(None);
                arguments.insert("message", message);
                self.web.call_async_javascript_function(
                    "window.dispatchEvent(new MessageEvent('message',{data:message}))",
                    Some(&arguments.end()),
                    None,
                    None,
                    None::<&Cancellable>,
                    |_| {},
                );
            }
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        self.window.close();
    }
}
