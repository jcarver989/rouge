cfg_select! {
    target_arch = "wasm32" => {
        pub mod web;
        pub(crate) use web::Web as Current;
        pub use web::Error;
    }
    target_os = "android" => {
        mod android;
        pub(crate) use android::Android as Current;
        pub use android::{AndroidApp, Error};
    }
    _ => {
        mod native;
        #[doc(hidden)]
        pub use native::{
            Wheel, WheelGesture, appearance, cursor_icon, logical_point, logical_size, modifiers,
            named_key, pointer_button, scroll_delta, touch_phase,
        };
        cfg_select! {
            target_os = "ios" => {
                mod ios;
                pub(crate) use ios::Ios as Current;
                pub use ios::Error;
            }
            target_os = "macos" => {
                mod macos;
                pub(crate) use macos::MacOs as Current;
                pub use macos::Error;
            }
            _ => {
                mod desktop;
                pub(crate) use desktop::Desktop as Current;
                pub use desktop::Error;
            }
        }
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
mod editing;
#[cfg(not(target_arch = "wasm32"))]
mod touch;

use crate::app::{App, AppError, RootFn};
use rouge_core::{AnyView, Cursor, Menu, Styled, View, div, safe_area, window_handle};
#[cfg(feature = "webview")]
use rouge_webview::{SystemEngine, WebEngine, provide_web_engine};
use rouge_widgets::{menu_bar, window_chrome};
#[cfg(all(feature = "persist", not(target_arch = "wasm32")))]
use std::env::var_os;
#[cfg(feature = "open-url")]
use std::io::Error as IoError;
#[cfg(all(feature = "persist", not(target_arch = "wasm32")))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub use touch::PrimaryTouch;

#[cfg(not(target_os = "android"))]
pub(crate) const LINE_SCROLL: f32 = 40.0;

pub(crate) type Options = <Current as Platform>::Options;

pub(crate) trait Platform {
    type Options: Default;

    const MOBILE: bool = false;
    const MENU_BAR: bool = true;
    const WINDOW_MENU: bool = false;

    fn run(app: App, options: Self::Options) -> Result<(), AppError>;

    fn log(message: &str) {
        eprintln!("{message}");
    }

    fn warn(message: &str) {
        eprintln!("rouge: {message}");
    }

    #[cfg(feature = "open-url")]
    fn open_url(url: &str) -> Result<(), IoError>;

    #[cfg(all(feature = "persist", not(target_arch = "wasm32")))]
    fn default_config_dir(name: &str) -> Option<PathBuf>;

    #[cfg(all(feature = "persist", not(target_arch = "wasm32")))]
    fn config_dir(name: &str) -> Option<PathBuf> {
        var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join(name.to_lowercase()))
            .or_else(|| Self::default_config_dir(name))
    }
}

pub(crate) fn log(message: &str) {
    Current::log(message);
}

pub(crate) fn warn(message: &str) {
    Current::warn(message);
}

#[cfg(feature = "open-url")]
pub fn open_url(url: &str) -> Result<(), IoError> {
    Current::open_url(url)
}

#[doc(hidden)]
pub fn css_cursor(cursor: Cursor) -> &'static str {
    match cursor {
        Cursor::Default => "default",
        Cursor::Pointer => "pointer",
        Cursor::Text => "text",
        Cursor::Grab => "grab",
        Cursor::Grabbing => "grabbing",
        Cursor::NotAllowed => "not-allowed",
        Cursor::Crosshair => "crosshair",
        Cursor::ResizeHorizontal => "ew-resize",
        Cursor::ResizeVertical => "ns-resize",
    }
}

pub(crate) fn default_menus(mut menus: Vec<Menu>) -> Vec<Menu> {
    let has = |menus: &[Menu], title: &str| menus.iter().any(|menu| menu.title == title);
    if !has(&menus, "Edit") {
        menus.insert(menus.len().min(1), Menu::edit());
    }
    if Current::WINDOW_MENU && !has(&menus, "Window") {
        menus.push(Menu::window());
    }
    menus
}

#[cfg(feature = "webview")]
pub(crate) fn web_engine<T>(host: T) -> impl FnOnce()
where
    SystemEngine<T>: WebEngine,
{
    move || provide_web_engine(SystemEngine(host))
}

#[cfg(not(feature = "webview"))]
pub(crate) fn web_engine<T>(_host: T) -> impl FnOnce() {
    || {}
}

pub(crate) fn window_root(
    title: String,
    menus: Vec<Menu>,
    chrome: bool,
    root: RootFn,
    provide_engine: impl FnOnce(),
) -> AnyView {
    provide_engine();
    let handle = window_handle();
    handle.set_title(title);
    let root = safe_area_root(menu_root(menus, root()));
    if chrome {
        window_chrome(handle.title(), root).into_any()
    } else {
        root
    }
}

fn safe_area_root(root: AnyView) -> AnyView {
    if !Current::MOBILE {
        return root;
    }
    let insets = safe_area();
    div()
        .size_full()
        .flex()
        .col()
        .pt(move || insets.get().top)
        .pr(move || insets.get().right)
        .pb(move || insets.get().bottom)
        .pl(move || insets.get().left)
        .child(div().flex_1().min_h(0.0).root_host().child(root))
        .into_any()
}

fn menu_root(menus: Vec<Menu>, root: AnyView) -> AnyView {
    if !Current::MENU_BAR || menus.is_empty() {
        return root;
    }
    div()
        .size_full()
        .flex()
        .col()
        .child(menu_bar(menus))
        .child(div().flex_1().min_h(0.0).root_host().child(root))
        .into_any()
}
