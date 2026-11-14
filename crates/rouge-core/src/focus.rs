use crate::motion::UiContext;
use crate::view::{Patch, Queue};
use rouge_reactive::{Signal, expect_context, signal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusHandle {
    id: u64,
    focused: Signal<bool>,
    queue: Signal<Queue>,
}

#[track_caller]
pub fn focus_handle() -> FocusHandle {
    let context = expect_context::<UiContext>();
    FocusHandle {
        id: context.next_id(),
        focused: signal(false),
        queue: signal(context.queue()),
    }
}

impl FocusHandle {
    pub(crate) fn binding(&self) -> (u64, Signal<bool>) {
        (self.id, self.focused)
    }

    fn request(&self, focus: bool) {
        if self.queue.is_alive() {
            let id = self.id;
            self.queue
                .with_untracked(|queue| queue.push(Patch::Focus(id, focus)));
        }
    }

    pub fn focus(&self) {
        self.request(true);
    }

    pub fn blur(&self) {
        self.request(false);
    }

    pub fn is_focused(&self) -> bool {
        self.focused.is_alive() && self.focused.get()
    }
}
