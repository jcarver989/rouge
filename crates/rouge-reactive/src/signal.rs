use crate::handle::Handle;
use crate::runtime::{Graph, NodeId, Value, with_rt};
use crate::sender::SignalSender;
use slotmap::Key;
use std::cell::RefCell;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::mem::replace;
use std::ops::Deref;
use std::rc::Rc;

pub struct ReadSignal<T> {
    handle: Handle,
    marker: PhantomData<*const T>,
}

pub struct Signal<T>(ReadSignal<T>);

impl<T> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ReadSignal<T> {}

impl<T> PartialEq for ReadSignal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.handle == other.handle
    }
}

impl<T> Eq for ReadSignal<T> {}

impl<T> Hash for ReadSignal<T> {
    fn hash<U: Hasher>(&self, state: &mut U) {
        self.handle.hash(state);
    }
}

impl<T> Debug for ReadSignal<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_tuple("ReadSignal").field(&self.handle).finish()
    }
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Signal<T> {}

impl<T> PartialEq for Signal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Signal<T> {}

impl<T> Hash for Signal<T> {
    fn hash<U: Hasher>(&self, state: &mut U) {
        self.0.hash(state);
    }
}

impl<T> Debug for Signal<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_tuple("Signal").field(&self.0).finish()
    }
}

#[track_caller]
pub fn signal<T: 'static>(value: T) -> Signal<T> {
    let value: Value = Rc::new(RefCell::new(Some(value)));
    Signal(ReadSignal::new(with_rt(|rt| {
        rt.create_node(rt.current_owner(), Some(value), None)
    })))
}

impl<T: 'static> ReadSignal<T> {
    #[track_caller]
    pub(crate) fn new(id: NodeId) -> Self {
        ReadSignal {
            handle: Handle::new(id),
            marker: PhantomData,
        }
    }

    #[track_caller]
    fn read<U>(&self, tracked: bool, f: impl FnOnce(&T) -> U) -> U {
        match self.handle.try_read(tracked, f) {
            Some(result) => result,
            None => self.handle.panic_disposed("signal or memo", "read"),
        }
    }

    #[track_caller]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.read(true, T::clone)
    }

    #[track_caller]
    pub fn get_untracked(&self) -> T
    where
        T: Clone,
    {
        self.read(false, T::clone)
    }

    #[track_caller]
    pub fn with<U>(&self, f: impl FnOnce(&T) -> U) -> U {
        self.read(true, f)
    }

    #[track_caller]
    pub fn with_untracked<U>(&self, f: impl FnOnce(&T) -> U) -> U {
        self.read(false, f)
    }

    pub fn try_get(&self) -> Option<T>
    where
        T: Clone,
    {
        self.handle.try_read(true, T::clone)
    }

    pub fn id(&self) -> u64 {
        self.handle.id.data().as_ffi()
    }

    pub fn is_alive(&self) -> bool {
        self.handle.is_alive()
    }
}

impl ReadSignal<bool> {
    pub fn pick<T: Clone + 'static>(self, on: T, off: T) -> impl Fn() -> T + Clone + 'static {
        move || {
            if self.get() { on.clone() } else { off.clone() }
        }
    }
}

impl Signal<bool> {
    pub fn toggle(&self) {
        self.update(bool::toggle);
    }
}

impl<T: Display + 'static> Display for ReadSignal<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        self.with(|value| value.fmt(f))
    }
}

impl<T: Display + 'static> Display for Signal<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Display::fmt(&self.0, f)
    }
}

impl<T> Deref for Signal<T> {
    type Target = ReadSignal<T>;

    fn deref(&self) -> &ReadSignal<T> {
        &self.0
    }
}

impl<T: 'static> Signal<T> {
    pub fn read_only(&self) -> ReadSignal<T> {
        self.0
    }

    #[track_caller]
    fn write<U>(&self, f: impl FnOnce(&mut T) -> U) -> Option<U> {
        let result = self.try_update(f);
        if result.is_none() && cfg!(debug_assertions) {
            self.handle.panic_disposed("signal", "write to");
        }
        result
    }

    #[track_caller]
    pub fn set(&self, value: T) {
        self.write(|v| replace(v, value));
    }

    #[track_caller]
    pub fn set_if_changed(&self, value: T) -> bool
    where
        T: PartialEq,
    {
        match self.handle.try_read(false, |v: &T| *v != value) {
            Some(false) => false,
            _ => self.write(|v| replace(v, value)).is_some(),
        }
    }

    #[track_caller]
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        self.write(f);
    }

    pub fn try_update<U>(&self, f: impl FnOnce(&mut T) -> U) -> Option<U> {
        self.handle.try_update(f)
    }

    pub fn try_set(&self, value: T) -> Result<(), T> {
        if !self.is_alive() {
            return Err(value);
        }
        self.set(value);
        Ok(())
    }

    #[track_caller]
    pub fn sender(&self) -> SignalSender<T> {
        SignalSender::new(self.handle, with_rt(Graph::mailbox))
    }
}
