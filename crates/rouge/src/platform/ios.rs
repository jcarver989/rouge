#[allow(unsafe_code)]
mod text_input;

use super::Platform;
use super::native::{UserEvent, WinitPlatform, run};
use crate::app::{App, AppError};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_foundation::NSString;
use objc2_ui_kit::{UIPasteboard, UITraitEnvironment, UIUserInterfaceStyle, UIView};
use rouge_core::{Appearance, Clipboard, ClipboardMedia, Menu, PasteRequest, Ui};
use rouge_render::Edges;
#[cfg(feature = "persist")]
use std::env::home_dir;
#[cfg(feature = "open-url")]
use std::io::Error as IoError;
#[cfg(feature = "persist")]
use std::path::PathBuf;
use thiserror::Error;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoopProxy;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

#[derive(Debug, Error)]
pub enum Error {}

#[derive(Default)]
pub(crate) struct Options;

pub(crate) struct Ios;

impl Platform for Ios {
    type Options = Options;

    const MOBILE: bool = true;
    const MENU_BAR: bool = false;

    fn run(app: App, _options: Self::Options) -> Result<(), AppError> {
        run(app)
    }

    #[cfg(feature = "open-url")]
    fn open_url(url: &str) -> Result<(), IoError> {
        open::that(url)
    }

    #[cfg(feature = "persist")]
    fn default_config_dir(name: &str) -> Option<PathBuf> {
        home_dir().map(|home| home.join("Library/Application Support").join(name))
    }
}

impl WinitPlatform for Ios {
    type Input = text_input::TextInput;

    fn new(_menus: &[Menu], _waker: &EventLoopProxy<UserEvent>) -> Result<Self, AppError> {
        Ok(Ios)
    }

    fn clipboard() -> impl Clipboard + 'static {
        IosClipboard::new()
    }

    fn appearance(window: &Window) -> Appearance {
        if MainThreadMarker::new().is_none() {
            return Appearance::Light;
        }
        let Ok(handle) = window.window_handle() else {
            return Appearance::Light;
        };
        let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
            return Appearance::Light;
        };
        #[allow(unsafe_code)]
        let dark = unsafe {
            // Winit lends its live UIView for the lifetime of the window handle.
            let view = handle.ui_view.cast::<UIView>().as_ref();
            view.traitCollection().userInterfaceStyle() == UIUserInterfaceStyle::Dark
        };
        if dark {
            Appearance::Dark
        } else {
            Appearance::Light
        }
    }

    fn sync(window: &Window, ui: &mut Ui) {
        ui.set_appearance(Self::appearance(window));
    }

    fn surface_size(window: &Window) -> PhysicalSize<u32> {
        window.outer_size()
    }

    fn safe_area(window: &Window) -> Edges {
        let (Ok(inner), Ok(outer)) = (window.inner_position(), window.outer_position()) else {
            return Edges::ZERO;
        };
        let (inner_size, outer_size) = (window.inner_size(), window.outer_size());
        let scale = window.scale_factor() as f32;
        let left = (inner.x - outer.x) as f32;
        let top = (inner.y - outer.y) as f32;
        let right = outer_size.width as f32 - inner_size.width as f32 - left;
        let bottom = outer_size.height as f32 - inner_size.height as f32 - top;
        Edges {
            top: (top / scale).max(0.0),
            right: (right / scale).max(0.0),
            bottom: (bottom / scale).max(0.0),
            left: (left / scale).max(0.0),
        }
    }
}

struct IosClipboard(Retained<UIPasteboard>);

impl IosClipboard {
    fn new() -> Self {
        IosClipboard(UIPasteboard::generalPasteboard())
    }

    #[allow(unsafe_code)]
    fn png(&self) -> Option<Vec<u8>> {
        if !unsafe { self.0.hasImages() } {
            return None;
        }
        let image = unsafe { self.0.image() }?;
        image.png_representation().map(|data| data.to_vec())
    }

    #[allow(unsafe_code)]
    fn text(&self) -> Option<String> {
        unsafe { self.0.string() }.map(|text| text.to_string())
    }
}

impl Clipboard for IosClipboard {
    #[allow(unsafe_code)]
    fn write(&mut self, text: &str) {
        unsafe { self.0.setString(Some(&NSString::from_str(text))) };
    }

    fn read(&mut self, request: PasteRequest) {
        if request.wants_media()
            && let Some(png) = self.png()
        {
            request.media(ClipboardMedia::Png(png));
        } else if let Some(text) = self.text() {
            request.text(text);
        }
    }
}
