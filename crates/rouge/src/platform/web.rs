mod clipboard;
mod host;
mod runtime;
mod trace;

pub use super::css_cursor;
pub use runtime::BrowserRuntime;

pub(crate) use host::run;

use rouge_core::Key;
#[cfg(feature = "open-url")]
use std::io::Error as IoError;

pub(crate) fn warn(message: &str) {
    web_sys::console::warn_1(&format!("rouge: {message}").into());
}

#[cfg(feature = "open-url")]
pub(crate) fn open_url(url: &str) -> Result<(), IoError> {
    let opened = web_sys::window()
        .and_then(|window| window.open_with_url_and_target(url, "_blank").ok())
        .flatten();
    opened
        .map(|_| ())
        .ok_or_else(|| IoError::other("the browser blocked the new tab"))
}

pub fn dom_key(key: &str, code: &str, shift: bool) -> Key {
    match key {
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Tab" => Key::Tab,
        " " => Key::Space,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        _ if key.chars().count() == 1 => {
            let base = shift.then(|| unshifted(code)).flatten();
            match base {
                Some(base) => Key::character(base),
                None => Key::character(&key.to_lowercase()),
            }
        }
        _ => key
            .strip_prefix('F')
            .and_then(|number| number.parse().ok())
            .map_or(Key::Other, Key::F),
    }
}

fn unshifted(code: &str) -> Option<&str> {
    if let Some(digit) = code.strip_prefix("Digit").filter(|rest| rest.len() == 1) {
        return Some(digit);
    }
    Some(match code {
        "Minus" => "-",
        "Equal" => "=",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" => "\\",
        "Semicolon" => ";",
        "Quote" => "'",
        "Backquote" => "`",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        _ => return None,
    })
}
