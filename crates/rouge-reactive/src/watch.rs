use crate::effect::{Effect, effect};
use crate::memo::memo;
use crate::scope::{Scope, run_in};
use std::cell::RefCell;

#[track_caller]
pub fn watch<T: Clone + PartialEq + 'static>(
    source: impl Fn() -> T + 'static,
    handler: impl Fn(&T, Option<&T>) + 'static,
) -> Effect {
    let owner = Scope::current();
    let value = memo(source);
    let previous = RefCell::new(None::<T>);
    effect(move || {
        let current = value.get();
        let last = previous.replace(Some(current.clone()));
        run_in(owner, || handler(&current, last.as_ref()));
    })
}
