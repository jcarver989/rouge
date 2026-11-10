use crate::handle::Handle;
use crate::runtime::{Compute, with_rt};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Effect {
    handle: Handle,
}

#[track_caller]
pub fn effect(f: impl FnMut() + 'static) -> Effect {
    let f = RefCell::new(f);
    let compute: Compute = Rc::new(move || {
        (f.borrow_mut())();
        true
    });
    let handle = Handle::new(with_rt(|rt| {
        rt.create_node(rt.current_owner(), None, Some(compute))
    }));
    with_rt(|rt| rt.update_if_necessary(handle.id));
    Effect { handle }
}

impl Effect {
    pub fn dispose(self) {
        self.handle.dispose();
    }

    pub fn is_alive(&self) -> bool {
        self.handle.is_alive()
    }
}
