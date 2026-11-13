use crate::scope::{Scope, run_in};
use crate::signal::{Signal, signal};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub struct Stored<T>(Signal<T>);

impl<T> Clone for Stored<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Stored<T> {}

impl<T> PartialEq for Stored<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Stored<T> {}

impl<T> Hash for Stored<T> {
    fn hash<U: Hasher>(&self, state: &mut U) {
        self.0.hash(state);
    }
}

impl<T> Debug for Stored<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_tuple("Stored").field(&self.0).finish()
    }
}

#[track_caller]
pub fn stored<T: 'static>(value: T) -> Stored<T> {
    Stored(signal(value))
}

impl<T: 'static> Stored<T> {
    #[track_caller]
    pub fn with<U>(&self, f: impl FnOnce(&T) -> U) -> U {
        self.0.with_untracked(f)
    }

    #[track_caller]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.0.get_untracked()
    }

    pub fn try_with<U>(&self, f: impl FnOnce(&T) -> U) -> Option<U> {
        self.0.is_alive().then(|| self.0.with_untracked(f))
    }

    #[track_caller]
    pub fn update<U>(&self, f: impl FnOnce(&mut T) -> U) -> U {
        match self.0.try_update(f) {
            Some(result) => result,
            None => {
                panic!("rouge-reactive: tried to update a stored value that was already disposed")
            }
        }
    }

    #[track_caller]
    pub fn set(&self, value: T) {
        self.update(|current| *current = value);
    }

    pub fn is_alive(&self) -> bool {
        self.0.is_alive()
    }
}

type Handler<T> = Rc<dyn Fn(T)>;

pub struct Callback<T = ()>(Option<Stored<Handler<T>>>);

impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Callback<T> {}

impl<T> Default for Callback<T> {
    fn default() -> Self {
        Callback(None)
    }
}

impl<T> PartialEq for Callback<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Callback<T> {}

impl<T> Debug for Callback<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_tuple("Callback").field(&self.0).finish()
    }
}

impl<T: 'static> Callback<T> {
    pub fn is_set(&self) -> bool {
        self.0.is_some()
    }

    pub fn is_alive(&self) -> bool {
        self.0.is_some_and(|handler| handler.is_alive())
    }

    fn invoke(&self, args: T) {
        let handler = self.0.and_then(|handler| handler.try_with(Rc::clone));
        if let Some(handler) = handler {
            handler(args);
        }
    }
}

impl Callback {
    pub fn call(&self) {
        self.invoke(());
    }
}

impl<T: 'static> Callback<(T,)> {
    pub fn call(&self, value: T) {
        self.invoke((value,));
    }
}

impl<T: Fn() + 'static> From<T> for Callback {
    #[track_caller]
    fn from(f: T) -> Self {
        let owner = Scope::current();
        Callback(Some(stored(Rc::new(move |()| {
            run_in(owner, &f);
        }))))
    }
}

impl<T: 'static, U: Fn(T) + 'static> From<U> for Callback<(T,)> {
    #[track_caller]
    fn from(f: U) -> Self {
        let owner = Scope::current();
        Callback(Some(stored(Rc::new(move |(value,)| {
            run_in(owner, || f(value));
        }))))
    }
}
