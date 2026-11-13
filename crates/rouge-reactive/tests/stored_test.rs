use rouge_reactive::{Callback, Runtime, Scope, effect, signal, stored};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn stored_values_are_copy_handles_that_do_not_subscribe_readers() {
    let _entered = Runtime::new().enter();
    let value = stored(vec![1]);
    let runs = Rc::new(Cell::new(0));
    effect({
        let runs = runs.clone();
        move || {
            value.with(Vec::len);
            runs.set(runs.get() + 1);
        }
    });
    value.update(|items| items.push(2));
    assert_eq!(value.get(), [1, 2]);
    assert_eq!(runs.get(), 1);
}

#[test]
fn stored_values_die_with_their_scope() {
    let _entered = Runtime::new().enter();
    let scope = Scope::root();
    let value = scope.run(|| stored(1));
    scope.dispose();
    assert!(!value.is_alive());
    assert_eq!(value.try_with(|value| *value), None);
}

#[test]
fn callbacks_are_copy_and_convert_from_closures_with_or_without_an_argument() {
    let _entered = Runtime::new().enter();
    let total = Rc::new(Cell::new(0));
    let bump: Callback = {
        let total = total.clone();
        (move || total.set(total.get() + 1)).into()
    };
    let add: Callback<(i32,)> = {
        let total = total.clone();
        (move |amount| total.set(total.get() + amount)).into()
    };
    let (first, second) = (bump, bump);
    first.call();
    second.call();
    add.call(5);
    assert_eq!(total.get(), 7);
    assert!(bump.is_set());
}

#[test]
fn unset_and_disposed_callbacks_do_nothing() {
    let _entered = Runtime::new().enter();
    let calls = Rc::new(Cell::new(0));
    let unset = Callback::<()>::default();
    unset.call();
    assert!(!unset.is_set());
    let scope = Scope::root();
    let disposed: Callback = scope.run(|| {
        let calls = calls.clone();
        (move || calls.set(calls.get() + 1)).into()
    });
    scope.dispose();
    disposed.call();
    assert_eq!(calls.get(), 0);
}

#[test]
fn select_notifies_only_when_the_selected_part_changes() {
    let _entered = Runtime::new().enter();
    let state = signal((1, "a"));
    let number = state.select(|state| state.0);
    let runs = Rc::new(Cell::new(0));
    effect({
        let runs = runs.clone();
        move || {
            number.get();
            runs.set(runs.get() + 1);
        }
    });
    state.update(|state| state.1 = "b");
    assert_eq!(runs.get(), 1);
    state.update(|state| state.0 = 2);
    assert_eq!((runs.get(), number.get()), (2, 2));
}

#[test]
fn callbacks_run_untracked_in_the_scope_that_created_them() {
    let _entered = Runtime::new().enter();
    let scope = Scope::root();
    let created = stored(None);
    let callback = scope.run(|| Callback::from(move || created.set(Some(signal(1)))));
    let read = signal(0);
    let runs = Rc::new(Cell::new(0));
    effect({
        let runs = runs.clone();
        move || {
            runs.set(runs.get() + 1);
            callback.call();
            read.get_untracked();
        }
    });
    let made = created.get().expect("the callback ran");
    scope.dispose();
    assert!(
        !made.is_alive(),
        "the signal belonged to the callback's scope"
    );
    assert!(!callback.is_alive());
    callback.call();
    assert_eq!(runs.get(), 1);
}
