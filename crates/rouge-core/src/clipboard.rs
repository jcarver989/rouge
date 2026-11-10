use std::cell::RefCell;
use std::rc::Rc;

pub trait Clipboard {
    fn read(&mut self) -> Option<String>;
    fn write(&mut self, text: &str);
}

#[derive(Clone, Debug, Default)]
pub struct MemoryClipboard(Rc<RefCell<Option<String>>>);

impl MemoryClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> Option<String> {
        self.0.borrow().clone()
    }

    pub fn set_text(&self, text: impl Into<String>) {
        *self.0.borrow_mut() = Some(text.into());
    }
}

impl Clipboard for MemoryClipboard {
    fn read(&mut self) -> Option<String> {
        self.text()
    }

    fn write(&mut self, text: &str) {
        self.set_text(text);
    }
}
