use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub struct ClipboardTarget(pub(crate) Rc<()>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardMedia {
    Files(Vec<PathBuf>),
    Png(Vec<u8>),
}

pub trait Clipboard {
    fn read(&mut self) -> Option<String>;
    fn write(&mut self, text: &str);

    fn read_media(&mut self) -> Option<ClipboardMedia> {
        None
    }
}

#[derive(Clone, Debug, Default)]
pub struct MemoryClipboard(Rc<RefCell<Contents>>);

#[derive(Debug, Default)]
struct Contents {
    text: Option<String>,
    media: Option<ClipboardMedia>,
}

impl MemoryClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> Option<String> {
        self.0.borrow().text.clone()
    }

    pub fn media(&self) -> Option<ClipboardMedia> {
        self.0.borrow().media.clone()
    }

    pub fn set_text(&self, text: impl Into<String>) {
        *self.0.borrow_mut() = Contents {
            text: Some(text.into()),
            media: None,
        };
    }

    pub fn set_media(&self, text: Option<String>, media: ClipboardMedia) {
        *self.0.borrow_mut() = Contents {
            text,
            media: Some(media),
        };
    }
}

impl Clipboard for MemoryClipboard {
    fn read(&mut self) -> Option<String> {
        self.text()
    }

    fn write(&mut self, text: &str) {
        self.set_text(text);
    }

    fn read_media(&mut self) -> Option<ClipboardMedia> {
        self.media()
    }
}
