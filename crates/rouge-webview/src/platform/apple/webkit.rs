use super::wk_web_view::WebKitView;
use crate::engine::{PageCommand, WebViewEvent, WebViewEvents, WebViewOptions};
use crate::platform::shared::inline::{Inline, SCHEME};
use crate::platform::shared::message_handler::{BRIDGE_NAME, BRIDGE_SCRIPT};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel,
};
use objc2_core_foundation::CGRect;
use objc2_foundation::{
    NSData, NSDictionary, NSError, NSInteger, NSKeyValueObservingOptions,
    NSObjectNSKeyValueObserverRegistration, NSString, NSURL, NSURLRequest, NSURLResponse,
    ns_string,
};
use objc2_web_kit::{
    WKContentWorld, WKScriptMessage, WKScriptMessageHandler, WKURLSchemeHandler, WKURLSchemeTask,
    WKUserContentController, WKUserScript, WKUserScriptInjectionTime, WKWebViewConfiguration,
};
use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::null_mut;

const OBSERVED: [&str; 6] = [
    "loading",
    "estimatedProgress",
    "URL",
    "title",
    "canGoBack",
    "canGoForward",
];
const CANCELLED: isize = -999;
const NOT_FOUND: isize = -1100;
const UNKNOWN_LENGTH: NSInteger = -1;

impl WebKitView {
    fn report(&self, events: &WebViewEvents, key: &str) {
        let event = match key {
            "loading" => WebViewEvent::Loading(self.is_loading()),
            "estimatedProgress" => WebViewEvent::Progress(self.estimated_progress() as f32),
            "URL" => {
                let url = self.url().and_then(|url| url.absoluteString());
                WebViewEvent::Url(url.map_or_default(|url| url.to_string()))
            }
            "title" => WebViewEvent::Title(self.title().map_or_default(|title| title.to_string())),
            "canGoBack" | "canGoForward" => WebViewEvent::History {
                back: self.can_go_back(),
                forward: self.can_go_forward(),
            },
            _ => return,
        };
        events.send(event);
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RougeWebObserver"]
    #[ivars = WebViewEvents]
    pub(super) struct Observer;

    unsafe impl NSObjectProtocol for Observer {}

    unsafe impl WKScriptMessageHandler for Observer {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn received(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
            // SAFETY: WebKit hands a live message on the main thread.
            let body = unsafe { message.body() };
            if let Ok(text) = body.downcast::<NSString>() {
                self.ivars().send(WebViewEvent::Message(text.to_string()));
            }
        }
    }

    impl Observer {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observed(
            &self,
            key: Option<&NSString>,
            web: Option<&AnyObject>,
            _change: Option<&AnyObject>,
            _context: *mut c_void,
        ) {
            let web = web.and_then(|web| web.downcast_ref::<WebKitView>());
            if let (Some(key), Some(web)) = (key, web) {
                web.report(self.ivars(), &key.to_string());
            }
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn failed(&self, _web: &AnyObject, _navigation: Option<&AnyObject>, error: &NSError) {
            self.fail(error);
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn failed_early(&self, _web: &AnyObject, _navigation: Option<&AnyObject>, error: &NSError) {
            self.fail(error);
        }
    }
);

impl Observer {
    fn new(mtm: MainThreadMarker, events: WebViewEvents) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(events);
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn fail(&self, error: &NSError) {
        if error.code() != CANCELLED {
            let reason = error.localizedDescription().to_string();
            self.ivars().send(WebViewEvent::Failed(reason));
        }
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RougeInlinePages"]
    #[ivars = RefCell<Option<Inline>>]
    struct InlinePages;

    unsafe impl NSObjectProtocol for InlinePages {}

    unsafe impl WKURLSchemeHandler for InlinePages {
        #[unsafe(method(webView:startURLSchemeTask:))]
        fn start(&self, _web: &AnyObject, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            self.serve(task);
        }

        #[unsafe(method(webView:stopURLSchemeTask:))]
        fn stop(&self, _web: &AnyObject, _task: &ProtocolObject<dyn WKURLSchemeTask>) {}
    }
);

impl InlinePages {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RefCell::new(None));
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn serve(&self, task: &ProtocolObject<dyn WKURLSchemeTask>) {
        let shown = self.ivars().borrow();
        // SAFETY: WebKit hands a live task on the main thread that it has not stopped, and the
        // task is answered once, with a response, its data and then the finish it expects.
        unsafe {
            let url = task.request().URL();
            let wanted = url.as_ref().and_then(|url| url.absoluteString());
            match (url, shown.as_ref()) {
                (Some(url), Some(inline))
                    if wanted.is_some_and(|wanted| wanted.to_string() == inline.address) =>
                {
                    let length =
                        NSInteger::try_from(inline.document.len()).unwrap_or(UNKNOWN_LENGTH);
                    let response =
                        NSURLResponse::initWithURL_MIMEType_expectedContentLength_textEncodingName(
                            NSURLResponse::alloc(),
                            &url,
                            Some(ns_string!("text/html")),
                            length,
                            Some(ns_string!("utf-8")),
                        );
                    task.didReceiveResponse(&response);
                    task.didReceiveData(&NSData::with_bytes(inline.document.as_bytes()));
                    task.didFinish();
                }
                _ => {
                    task.didFailWithError(&NSError::new(NOT_FOUND, ns_string!("NSURLErrorDomain")));
                }
            }
        }
    }
}

pub(super) struct WebKit {
    view: Retained<WebKitView>,
    observer: Retained<Observer>,
    controller: Retained<WKUserContentController>,
    inline: Retained<InlinePages>,
}

impl WebKit {
    pub(super) fn open(
        mtm: MainThreadMarker,
        options: WebViewOptions,
        events: WebViewEvents,
    ) -> Self {
        let observer = Observer::new(mtm, events);
        let inline = InlinePages::new(mtm);
        // SAFETY: WebKit objects are created and configured on the main thread, with live
        // arguments of the types each selector declares.
        let (configuration, controller) = unsafe {
            let configuration = WKWebViewConfiguration::new(mtm);
            configuration.setURLSchemeHandler_forURLScheme(
                Some(ProtocolObject::from_ref(&*inline)),
                &NSString::from_str(SCHEME),
            );
            let controller = configuration.userContentController();
            let script = WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                WKUserScript::alloc(mtm),
                &NSString::from_str(BRIDGE_SCRIPT),
                WKUserScriptInjectionTime::AtDocumentStart,
                true,
            );
            controller.addUserScript(&script);
            controller.addScriptMessageHandler_name(
                ProtocolObject::from_ref(&*observer),
                &NSString::from_str(BRIDGE_NAME),
            );
            (configuration, controller)
        };
        let view = WebKitView::init(WebKitView::alloc(mtm), CGRect::ZERO, &configuration);
        view.set_navigation_delegate(Some(&observer));
        for key in OBSERVED {
            // SAFETY: the observer outlives the registration, which `Drop` removes.
            unsafe {
                view.addObserver_forKeyPath_options_context(
                    &observer,
                    &NSString::from_str(key),
                    NSKeyValueObservingOptions::New,
                    null_mut(),
                );
            }
        }
        if options.inspectable && view.responds_to(sel!(setInspectable:)) {
            view.set_inspectable(true);
        }
        WebKit {
            view,
            observer,
            controller,
            inline,
        }
    }

    pub(super) fn view(&self) -> &WebKitView {
        &self.view
    }

    pub(super) fn load_url(&self, url: &str) -> bool {
        let url = NSURL::URLWithString(&NSString::from_str(url));
        if let Some(url) = &url {
            self.view.load_request(&NSURLRequest::requestWithURL(url));
        }
        url.is_some()
    }

    pub(super) fn load_inline(&self, inline: Inline) -> bool {
        let address = inline.address.clone();
        self.inline.ivars().replace(Some(inline));
        self.load_url(&address)
    }

    pub(super) fn run(&self, command: PageCommand) {
        match command {
            PageCommand::Reload => drop(self.view.reload()),
            PageCommand::Stop => self.view.stop_loading(),
            PageCommand::Back => drop(self.view.go_back()),
            PageCommand::Forward => drop(self.view.go_forward()),
            PageCommand::Post(message) => self.post(&message),
            PageCommand::Load(_) | PageCommand::Place(_) => {}
        }
    }
}

impl WebKit {
    fn post(&self, message: &str) {
        let message = NSString::from_str(message);
        let arguments = NSDictionary::<NSString, AnyObject>::from_slices(
            &[ns_string!("message")],
            &[message.as_ref()],
        );
        // SAFETY: the page world is read on the main thread this view lives on.
        let world = unsafe { WKContentWorld::pageWorld(self.view.mtm()) };
        self.view.call_async_java_script(
            ns_string!("window.dispatchEvent(new MessageEvent('message',{data:message}))"),
            Some(&arguments),
            None,
            &world,
            None,
        );
    }
}

impl Drop for WebKit {
    fn drop(&mut self) {
        for key in OBSERVED {
            // SAFETY: removes a registration made in `open`, while both objects are alive.
            unsafe {
                self.view
                    .removeObserver_forKeyPath(&self.observer, &NSString::from_str(key));
            }
        }
        self.view.set_navigation_delegate(None);
        // SAFETY: removes the handler added in `open`, on the main thread.
        unsafe {
            self.controller
                .removeScriptMessageHandlerForName(&NSString::from_str(BRIDGE_NAME));
        }
    }
}
