use rouge_core::{Div, FocusHandle, Key, focus_handle};
use std::rc::Rc;

#[derive(Clone)]
pub struct RovingFocus(Rc<[FocusHandle]>);

impl RovingFocus {
    pub fn new(count: usize) -> Self {
        RovingFocus((0..count).map(|_| focus_handle()).collect())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn handle(&self, index: usize) -> FocusHandle {
        self.0[index]
    }

    pub fn step(&self, from: usize, key: &Key) -> bool {
        let count = self.0.len();
        let target = match key {
            Key::ArrowDown => (from + 1) % count,
            Key::ArrowUp => (from + count - 1) % count,
            Key::Home => 0,
            Key::End => count - 1,
            _ => return false,
        };
        self.0[target].focus();
        true
    }

    pub fn item(&self, index: usize, row: Div) -> Div {
        let focus = self.clone();
        let row = row.track_focus(self.0[index]).on_key_down(move |event| {
            if focus.step(index, &event.key) {
                event.stop_propagation();
            }
        });
        if index == 0 { row.autofocus() } else { row }
    }
}
