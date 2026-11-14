use rouge_core::{Clipboard, PasteRequest};
use rouge_reactive::spawn;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen_futures::JsFuture;

#[derive(Clone, Default)]
pub(crate) struct WebClipboard(Rc<RefCell<Exchange>>);

#[derive(Default)]
struct Exchange {
    pasted: Option<String>,
    copied: Option<String>,
    capturing: bool,
}

impl WebClipboard {
    pub(crate) fn paste_from(&self, text: String, paste: impl FnOnce() -> bool) -> bool {
        self.0.borrow_mut().pasted = Some(text);
        let handled = paste();
        self.0.borrow_mut().pasted = None;
        handled
    }

    pub(crate) fn copy_into(&self, copy: impl FnOnce() -> bool) -> Option<String> {
        self.0.borrow_mut().capturing = true;
        copy();
        let mut exchange = self.0.borrow_mut();
        exchange.capturing = false;
        exchange.copied.take()
    }
}

impl Clipboard for WebClipboard {
    fn write(&mut self, text: &str) {
        let mut exchange = self.0.borrow_mut();
        if exchange.capturing {
            exchange.copied = Some(text.to_owned());
            return;
        }
        let Some(clipboard) = web_sys::window().map(|window| window.navigator().clipboard()) else {
            return;
        };
        let written = JsFuture::from(clipboard.write_text(text));
        spawn(async move {
            if let Err(error) = written.await {
                web_sys::console::warn_1(&error);
            }
        });
    }

    fn read(&mut self, request: PasteRequest) {
        if let Some(text) = self.0.borrow_mut().pasted.take() {
            request.text(text);
            return;
        }
        let Some(clipboard) = web_sys::window().map(|window| window.navigator().clipboard()) else {
            return;
        };
        let read = JsFuture::from(clipboard.read_text());
        spawn(async move {
            match read.await {
                Ok(text) => {
                    if let Some(text) = text.as_string() {
                        request.text(text);
                    }
                }
                Err(error) => web_sys::console::warn_1(&error),
            }
        });
    }
}
