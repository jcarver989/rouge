#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub use native::{
    PrimaryTouch, Wheel, WheelGesture, appearance, cursor_icon, logical_point, logical_size,
    modifiers, named_key, pointer_button, scroll_delta,
};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{log, run, warn};
#[cfg(target_arch = "wasm32")]
pub(crate) use web::{log, run, warn};

use crate::app::RootFn;
#[cfg(target_os = "ios")]
use rouge_core::safe_area;
use rouge_core::{AnyView, Cursor, Menu, View, window_handle};
#[cfg(not(target_os = "macos"))]
use rouge_core::{Styled, div};
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
use rouge_widgets::menu_bar;
use rouge_widgets::window_chrome;
#[cfg(feature = "open-url")]
use std::io::Error as IoError;

pub(crate) const LINE_SCROLL: f32 = 40.0;

#[cfg(feature = "open-url")]
pub fn open_url(url: &str) -> Result<(), IoError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        open::that(url)
    }
    #[cfg(target_arch = "wasm32")]
    {
        web::open_url(url)
    }
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
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    if !has(&menus, "Window") {
        menus.push(Menu::window());
    }
    menus
}

pub(crate) fn window_root(title: String, menus: Vec<Menu>, chrome: bool, root: RootFn) -> AnyView {
    let handle = window_handle();
    handle.set_title(title);
    let root = safe_area_root(menu_root(menus, root()));
    if chrome {
        window_chrome(handle.title(), root).into_any()
    } else {
        root
    }
}

#[cfg(target_os = "ios")]
fn safe_area_root(root: AnyView) -> AnyView {
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

#[cfg(not(target_os = "ios"))]
fn safe_area_root(root: AnyView) -> AnyView {
    root
}

fn menu_root(menus: Vec<Menu>, root: AnyView) -> AnyView {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let _ = menus;
        root
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        if menus.is_empty() {
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
}
