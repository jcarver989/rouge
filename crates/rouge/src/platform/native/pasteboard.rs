#![allow(unsafe_code)]

use objc2::rc::Retained;
use objc2_foundation::NSString;
use objc2_ui_kit::UIPasteboard;
use rouge_core::{Clipboard, ClipboardMedia, PasteRequest};

pub(crate) struct NativeClipboard(Retained<UIPasteboard>);

impl NativeClipboard {
    pub(crate) fn new() -> Self {
        NativeClipboard(UIPasteboard::generalPasteboard())
    }

    fn png(&self) -> Option<Vec<u8>> {
        if !unsafe { self.0.hasImages() } {
            return None;
        }
        let image = unsafe { self.0.image() }?;
        image.png_representation().map(|data| data.to_vec())
    }

    fn text(&self) -> Option<String> {
        unsafe { self.0.string() }.map(|text| text.to_string())
    }
}

impl Clipboard for NativeClipboard {
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
