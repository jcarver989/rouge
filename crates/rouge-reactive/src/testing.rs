use crate::runtime::{Entered, Runtime};
use crate::scope::Scope;
use crate::task::poll_tasks;
use std::cell::{Cell, RefCell};
use std::error::Error;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

pub type TestResult = Result<(), Box<dyn Error>>;

pub fn counter() -> (Rc<Cell<u32>>, impl Fn() + Clone + 'static) {
    let count = Rc::new(Cell::new(0));
    let c = count.clone();
    (count, move || c.set(c.get() + 1))
}

pub fn recorder<T: 'static>() -> (Rc<RefCell<Vec<T>>>, impl Fn(T) + Clone + 'static) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    (seen, move |value| s.borrow_mut().push(value))
}

pub fn in_root(f: impl FnOnce()) {
    let _entered = Runtime::new().enter();
    let root = Scope::root();
    root.run(f);
    root.dispose();
}

pub struct Driver {
    _entered: Entered,
    woken: Receiver<()>,
}

impl Default for Driver {
    fn default() -> Self {
        Driver::new()
    }
}

impl Driver {
    pub fn new() -> Driver {
        let (tx, woken) = channel();
        let runtime = Runtime::with_waker(move || {
            let _ = tx.send(());
        });
        Driver {
            _entered: runtime.enter(),
            woken,
        }
    }

    pub fn wakes(&self) -> usize {
        self.woken.try_iter().count()
    }

    pub fn wait_for_wake(&self) -> Result<(), RecvTimeoutError> {
        self.woken.recv_timeout(TIMEOUT)
    }

    pub fn run_until(&self, done: impl Fn() -> bool) -> Result<(), RecvTimeoutError> {
        loop {
            poll_tasks();
            if done() {
                return Ok(());
            }
            self.wait_for_wake()?;
        }
    }
}
