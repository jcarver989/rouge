use arboard::Clipboard as SystemClipboard;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use rouge_core::{Clipboard, ClipboardMedia, PasteRequest};

pub(crate) struct DesktopClipboard(Option<SystemClipboard>);

impl DesktopClipboard {
    pub(crate) fn new() -> Self {
        DesktopClipboard(SystemClipboard::new().ok())
    }

    fn media(&mut self) -> Option<ClipboardMedia> {
        let clipboard = self.0.as_mut()?;
        if let Some(files) = clipboard
            .get()
            .file_list()
            .ok()
            .filter(|files| !files.is_empty())
        {
            return Some(ClipboardMedia::Files(files));
        }
        let image = clipboard.get_image().ok()?;
        let (width, height) = (
            u32::try_from(image.width).ok()?,
            u32::try_from(image.height).ok()?,
        );
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&image.bytes, width, height, ExtendedColorType::Rgba8)
            .ok()?;
        Some(ClipboardMedia::Png(png))
    }
}

impl Clipboard for DesktopClipboard {
    fn write(&mut self, text: &str) {
        if let Some(clipboard) = &mut self.0 {
            let _ = clipboard.set_text(text);
        }
    }

    fn read(&mut self, request: PasteRequest) {
        if request.wants_media()
            && let Some(media) = self.media()
        {
            request.media(media);
        } else if let Some(text) = self
            .0
            .as_mut()
            .and_then(|clipboard| clipboard.get_text().ok())
        {
            request.text(text);
        }
    }
}
