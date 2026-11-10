use crate::runtime::{Compute, Value, with_rt};
use crate::signal::ReadSignal;
use std::cell::RefCell;
use std::rc::Rc;

pub type Memo<T> = ReadSignal<T>;

#[track_caller]
pub fn memo<T: PartialEq + 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    let slot = Rc::new(RefCell::new(None));
    let compute: Compute = {
        let slot = slot.clone();
        Rc::new(move || {
            let new = Some(f());
            if *slot.borrow() == new {
                return false;
            }
            slot.replace(new);
            true
        })
    };
    let value: Value = slot;
    ReadSignal::new(with_rt(|rt| {
        rt.create_node(rt.current_owner(), Some(value), Some(compute))
    }))
}
