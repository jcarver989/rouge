use super::host::{Responder, View};
use super::webkit::Observer;
use block2::DynBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{MainThreadOnly, extern_class, extern_methods};
#[cfg(target_os = "macos")]
use objc2_core_foundation::CGFloat;
use objc2_core_foundation::CGRect;
use objc2_foundation::{NSDictionary, NSError, NSString, NSURL, NSURLRequest};
use objc2_web_kit::{WKContentWorld, WKWebViewConfiguration};

type Called = DynBlock<dyn Fn(*mut AnyObject, *mut NSError)>;

extern_class!(
    #[unsafe(super(View, Responder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "WKWebView"]
    pub(in crate::platform::apple) struct WebKitView;
);

impl WebKitView {
    extern_methods!(
        #[unsafe(method(initWithFrame:configuration:))]
        pub(in crate::platform::apple) fn init(
            this: Allocated<Self>,
            frame: CGRect,
            configuration: &WKWebViewConfiguration,
        ) -> Retained<Self>;

        #[unsafe(method(setNavigationDelegate:))]
        pub(in crate::platform::apple) fn set_navigation_delegate(
            &self,
            delegate: Option<&Observer>,
        );

        #[unsafe(method(respondsToSelector:))]
        pub(in crate::platform::apple) fn responds_to(&self, selector: Sel) -> bool;

        #[unsafe(method(setInspectable:))]
        pub(in crate::platform::apple) fn set_inspectable(&self, inspectable: bool);

        #[unsafe(method(loadRequest:))]
        pub(in crate::platform::apple) fn load_request(
            &self,
            request: &NSURLRequest,
        ) -> Option<Retained<NSObject>>;

        #[unsafe(method(reload))]
        pub(in crate::platform::apple) fn reload(&self) -> Option<Retained<NSObject>>;

        #[unsafe(method(stopLoading))]
        pub(in crate::platform::apple) fn stop_loading(&self);

        #[unsafe(method(goBack))]
        pub(in crate::platform::apple) fn go_back(&self) -> Option<Retained<NSObject>>;

        #[unsafe(method(goForward))]
        pub(in crate::platform::apple) fn go_forward(&self) -> Option<Retained<NSObject>>;

        #[unsafe(method(callAsyncJavaScript:arguments:inFrame:inContentWorld:completionHandler:))]
        pub(in crate::platform::apple) fn call_async_java_script(
            &self,
            body: &NSString,
            arguments: Option<&NSDictionary<NSString, AnyObject>>,
            frame: Option<&AnyObject>,
            world: &WKContentWorld,
            done: Option<&Called>,
        );

        #[unsafe(method(isLoading))]
        pub(in crate::platform::apple) fn is_loading(&self) -> bool;

        #[unsafe(method(estimatedProgress))]
        pub(in crate::platform::apple) fn estimated_progress(&self) -> f64;

        #[unsafe(method(URL))]
        pub(in crate::platform::apple) fn url(&self) -> Option<Retained<NSURL>>;

        #[unsafe(method(title))]
        pub(in crate::platform::apple) fn title(&self) -> Option<Retained<NSString>>;

        #[unsafe(method(canGoBack))]
        pub(in crate::platform::apple) fn can_go_back(&self) -> bool;

        #[unsafe(method(canGoForward))]
        pub(in crate::platform::apple) fn can_go_forward(&self) -> bool;

        #[cfg(target_os = "macos")]
        #[unsafe(method(setPageZoom:))]
        pub(in crate::platform::apple) fn set_page_zoom(&self, zoom: CGFloat);

        #[cfg(target_os = "ios")]
        #[unsafe(method(endEditing:))]
        pub(in crate::platform::apple) fn end_editing(&self, force: bool) -> bool;
    );
}
