use crate::tree::ElementId;
use crate::view::{Patch, Queue};
use std::cell::RefCell;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::mem::take;
use std::path::PathBuf;
use std::rc::Rc;

pub(crate) struct ClipboardTarget(pub(crate) Rc<()>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardMedia {
    Files(Vec<PathBuf>),
    Png(Vec<u8>),
}

pub trait Clipboard {
    fn write(&mut self, text: &str);
    fn read(&mut self, request: PasteRequest);
}

pub struct PasteRequest {
    pub(crate) element: ElementId,
    pub(crate) target: ClipboardTarget,
    pub(crate) media: bool,
    pub(crate) queue: Queue,
}

pub(crate) enum Pasted {
    Text(String),
    Media(ClipboardMedia),
}

impl PasteRequest {
    pub fn wants_media(&self) -> bool {
        self.media
    }

    pub fn text(self, text: String) {
        self.deliver(Pasted::Text(text));
    }

    pub fn media(self, media: ClipboardMedia) {
        self.deliver(Pasted::Media(media));
    }

    fn deliver(self, pasted: Pasted) {
        let PasteRequest {
            element,
            target,
            queue,
            ..
        } = self;
        queue.push(Patch::Paste(element, target, pasted));
    }
}

#[derive(Clone, Debug, Default)]
pub struct MemoryClipboard(Rc<RefCell<Contents>>);

#[derive(Default)]
struct Contents {
    text: Option<String>,
    media: Option<ClipboardMedia>,
    deferred: bool,
    requests: Vec<PasteRequest>,
}

impl Debug for Contents {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Contents")
            .field("text", &self.text)
            .field("media", &self.media)
            .field("deferred", &self.deferred)
            .field("requests", &self.requests.len())
            .finish()
    }
}

impl MemoryClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn deferred() -> Self {
        let clipboard = Self::default();
        clipboard.0.borrow_mut().deferred = true;
        clipboard
    }

    pub fn text(&self) -> Option<String> {
        self.0.borrow().text.clone()
    }

    pub fn media(&self) -> Option<ClipboardMedia> {
        self.0.borrow().media.clone()
    }

    pub fn set_text(&self, text: impl Into<String>) {
        let mut contents = self.0.borrow_mut();
        contents.text = Some(text.into());
        contents.media = None;
    }

    pub fn set_media(&self, text: Option<String>, media: ClipboardMedia) {
        let mut contents = self.0.borrow_mut();
        contents.text = text;
        contents.media = Some(media);
    }

    pub fn pending(&self) -> usize {
        self.0.borrow().requests.len()
    }

    pub fn answer(&self) {
        let requests = take(&mut self.0.borrow_mut().requests);
        for request in requests {
            self.reply(request);
        }
    }

    fn reply(&self, request: PasteRequest) {
        let (text, media) = {
            let contents = self.0.borrow();
            (contents.text.clone(), contents.media.clone())
        };
        match (media.filter(|_| request.wants_media()), text) {
            (Some(media), _) => request.media(media),
            (None, Some(text)) => request.text(text),
            (None, None) => {}
        }
    }
}

impl Clipboard for MemoryClipboard {
    fn write(&mut self, text: &str) {
        self.set_text(text);
    }

    fn read(&mut self, request: PasteRequest) {
        let deferred = self.0.borrow().deferred;
        if deferred {
            self.0.borrow_mut().requests.push(request);
        } else {
            self.reply(request);
        }
    }
}
