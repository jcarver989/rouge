use super::Platform;
use super::native::clipboard::DesktopClipboard;
use super::native::{WinitPlatform, run};
use crate::app::{App, AppError};
use rouge_core::{Clipboard, Menu};
#[cfg(feature = "persist")]
use std::env::{home_dir, var_os};
#[cfg(feature = "open-url")]
use std::io::Error as IoError;
#[cfg(feature = "persist")]
use std::path::PathBuf;
use thiserror::Error;
use winit::event::KeyEvent;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::Key;
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

#[derive(Debug, Error)]
pub enum Error {}

#[derive(Default)]
pub(crate) struct Options;

pub(crate) struct Desktop;

impl Platform for Desktop {
    type Options = Options;

    const WINDOW_MENU: bool = true;

    fn run(app: App, options: Self::Options) -> Result<(), AppError> {
        run(app, options)
    }

    #[cfg(feature = "open-url")]
    fn open_url(url: &str) -> Result<(), IoError> {
        open::that(url)
    }
}

impl WinitPlatform for Desktop {
    fn new(_menus: &[Menu], _waker: &EventLoopProxy<()>) -> Result<Self, AppError> {
        Ok(Desktop)
    }

    fn clipboard() -> impl Clipboard + 'static {
        DesktopClipboard::new()
    }

    #[cfg(feature = "persist")]
    fn default_config_dir(name: &str) -> Option<PathBuf> {
        if cfg!(target_os = "windows") {
            var_os("APPDATA").map(|root| PathBuf::from(root).join(name))
        } else {
            home_dir().map(|home| home.join(".config").join(name.to_lowercase()))
        }
    }

    fn key(event: &KeyEvent) -> Key {
        event.key_without_modifiers()
    }
}
