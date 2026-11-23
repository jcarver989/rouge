use super::Platform;
use super::native::clipboard::DesktopClipboard;
use super::native::{UserEvent, WinitInput, WinitPlatform, run};
use crate::app::{App, AppError};
use image::imageops::FilterType;
use image::{ImageFormat, load_from_memory_with_format};
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
use winit::window::Icon;

#[derive(Debug, Error)]
pub enum Error {}

#[derive(Default)]
pub(crate) struct Options;

pub(crate) struct Desktop;

impl Platform for Desktop {
    type Options = Options;

    const WINDOW_MENU: bool = true;

    fn run(app: App, _options: Self::Options) -> Result<(), AppError> {
        run(app)
    }

    #[cfg(feature = "open-url")]
    fn open_url(url: &str) -> Result<(), IoError> {
        open::that(url)
    }

    #[cfg(feature = "persist")]
    fn default_config_dir(name: &str) -> Option<PathBuf> {
        if cfg!(target_os = "windows") {
            var_os("APPDATA").map(|root| PathBuf::from(root).join(name))
        } else {
            home_dir().map(|home| home.join(".config").join(name.to_lowercase()))
        }
    }
}

impl WinitPlatform for Desktop {
    type Input = WinitInput;

    fn new(_menus: &[Menu], _waker: &EventLoopProxy<UserEvent>) -> Result<Self, AppError> {
        Ok(Desktop)
    }

    fn clipboard() -> impl Clipboard + 'static {
        DesktopClipboard::new()
    }

    fn app_icon(png: &[u8]) -> Option<Icon> {
        let image = load_from_memory_with_format(png, ImageFormat::Png)
            .inspect_err(|error| Self::warn(&format!("invalid app icon: {error}")))
            .ok()?;
        let image = image.resize(128, 128, FilterType::Lanczos3).into_rgba8();
        let (width, height) = image.dimensions();
        Icon::from_rgba(image.into_raw(), width, height).ok()
    }

    fn key(event: &KeyEvent) -> Key {
        event.key_without_modifiers()
    }
}
