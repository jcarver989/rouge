use rouge_reactive::{Entered, Runtime};
use slotmap::{SlotMap, new_key_type};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

new_key_type! {
    struct WakeId;
}

struct Wake {
    pending: bool,
    callback: Rc<dyn Fn()>,
}

thread_local! {
    static WAKES: RefCell<SlotMap<WakeId, Wake>> = RefCell::new(SlotMap::with_key());
}

pub struct BrowserRuntime {
    runtime: Runtime,
    id: WakeId,
}

impl BrowserRuntime {
    pub fn new(wake: impl Fn() + 'static) -> Self {
        let id = WAKES.with_borrow_mut(|wakes| {
            wakes.insert(Wake {
                pending: false,
                callback: Rc::new(wake),
            })
        });
        let runtime = Runtime::with_waker(move || schedule(id));
        Self { runtime, id }
    }

    pub fn enter(&self) -> Entered {
        self.runtime.enter()
    }
}

impl Drop for BrowserRuntime {
    fn drop(&mut self) {
        let wake = WAKES.with_borrow_mut(|wakes| wakes.remove(self.id));
        drop(wake);
    }
}

fn schedule(id: WakeId) {
    let queued = WAKES.with_borrow_mut(|wakes| {
        wakes.get_mut(id).is_some_and(|wake| {
            if wake.pending {
                return false;
            }
            wake.pending = true;
            true
        })
    });
    if queued {
        spawn_local(async move {
            let callback = WAKES.with_borrow_mut(|wakes| {
                wakes.get_mut(id).map(|wake| {
                    wake.pending = false;
                    wake.callback.clone()
                })
            });
            if let Some(callback) = callback {
                callback();
            }
        });
    }
}
