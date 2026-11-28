use jni::bind_java_type;

bind_java_type! {
    pub(super) JavaPage => rouge.webview.WebPage,
    constructors {
        fn new(activity: android.app.Activity, peer: jlong, inspectable: bool),
    },
    methods {
        fn load_url(url: JString),
        fn load_html(html: JString, base_url: JString),
        fn place(interactive: bool, opacity: jfloat, frames: jint[], zoom: jfloat, radii: jfloat[]),
        fn hide(interactive: bool),
        fn reload(),
        fn stop(),
        fn go_back(),
        fn go_forward(),
        fn post(message: JString),
        fn close(),
    },
    native_methods_export = false,
    native_methods {
        static fn on_loading(peer: jlong, loading: bool),
        static fn on_progress(peer: jlong, progress: jfloat),
        static fn on_url(peer: jlong, url: JString),
        static fn on_title(peer: jlong, title: JString),
        static fn on_history(peer: jlong, back: bool, forward: bool),
        static fn on_failed(peer: jlong, reason: JString),
        static fn on_message(peer: jlong, message: JString),
        static fn on_closed(peer: jlong),
    },
}
