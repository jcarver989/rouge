#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{
    Wheel, WheelGesture, appearance, cursor_icon, logical_point, logical_size, modifiers,
    named_key, pointer_button, scroll_delta,
};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{run, warn};
#[cfg(target_arch = "wasm32")]
pub(crate) use web::{run, warn};

use crate::app::RootFn;
use rouge_core::{AnyView, Cursor, Menu, View, window_handle};
#[cfg(not(target_os = "macos"))]
use rouge_core::{Styled, div};
#[cfg(not(target_os = "macos"))]
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

pub(crate) fn window_root(title: String, menus: Vec<Menu>, chrome: bool, root: RootFn) -> AnyView {
    let handle = window_handle();
    handle.set_title(title);
    let root = menu_root(menus, root());
    if chrome {
        window_chrome(handle.title(), root).into_any()
    } else {
        root
    }
}

fn menu_root(menus: Vec<Menu>, root: AnyView) -> AnyView {
    #[cfg(target_os = "macos")]
    {
        let _ = menus;
        root
    }
    #[cfg(not(target_os = "macos"))]
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
