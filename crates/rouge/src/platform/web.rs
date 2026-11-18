mod clipboard;
mod host;
mod trace;

#[doc(hidden)]
pub use super::css_cursor;

pub use host::Mounted;

use super::Platform;
use crate::app::{App, AppError};
use host::{mount, run};
use rouge_core::Key;
#[cfg(feature = "open-url")]
use std::io::Error as IoError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {}

impl App {
    pub async fn mount(self) -> Result<Mounted, AppError> {
        mount(self).await
    }
}

#[derive(Default)]
pub(crate) struct Options;

pub(crate) struct Web;

impl Platform for Web {
    type Options = Options;

    fn run(app: App, _options: Self::Options) -> Result<(), AppError> {
        run(app)
    }

    fn log(message: &str) {
        web_sys::console::log_1(&message.into());
    }

    fn warn(message: &str) {
        web_sys::console::warn_1(&format!("rouge: {message}").into());
    }

    #[cfg(feature = "open-url")]
    fn open_url(url: &str) -> Result<(), IoError> {
        let opened = web_sys::window()
            .and_then(|window| window.open_with_url_and_target(url, "_blank").ok())
            .flatten();
        opened
            .map(|_| ())
            .ok_or_else(|| IoError::other("the browser blocked the new tab"))
    }
}

#[doc(hidden)]
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
