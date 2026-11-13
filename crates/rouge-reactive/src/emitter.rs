use crate::stored::{Callback, Stored, stored};
use std::fmt::{Debug, Formatter, Result as FmtResult};

pub struct Emitter<T: 'static>(Stored<Vec<Callback<(T,)>>>);

impl<T> Clone for Emitter<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Emitter<T> {}

impl<T> PartialEq for Emitter<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Emitter<T> {}

impl<T> Debug for Emitter<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_tuple("Emitter").field(&self.0).finish()
    }
}

#[track_caller]
pub fn emitter<T: 'static>() -> Emitter<T> {
    Emitter(stored(Vec::new()))
}

impl<T: Clone + 'static> Emitter<T> {
    #[track_caller]
    pub fn subscribe(&self, listener: impl Fn(T) + 'static) {
        let listener = Callback::from(listener);
        self.0.update(|listeners| {
            listeners.retain(Callback::is_alive);
            listeners.push(listener);
        });
    }

    pub fn emit(&self, value: T) {
        let Some(listeners) = self.0.try_with(Clone::clone) else {
            return;
        };
        for listener in listeners.iter().filter(|listener| listener.is_alive()) {
            listener.call(value.clone());
        }
    }
}
