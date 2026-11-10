use rouge_reactive::testing::counter;
use rouge_reactive::{Runtime, Scope, effect, on_cleanup, signal};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
#[should_panic(expected = "no runtime is entered")]
fn signals_need_an_entered_runtime() {
    let runtime = Runtime::new();
    drop(runtime.enter());
    signal(1);
}

#[test]
fn handles_work_again_when_the_runtime_is_re_entered() {
    let runtime = Runtime::new();
    let s = {
        let _entered = runtime.enter();
        signal(1)
    };
    assert!(
        !s.is_alive(),
        "nothing is alive while no runtime is entered"
    );
    let _entered = runtime.enter();
    s.set(2);
    assert_eq!(s.get(), 2);
}

#[test]
fn dropping_the_runtime_disposes_everything_it_owns() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (runs, bump) = counter();
    let runtime = Runtime::new();
    let entered = runtime.enter();
    let orphan = signal(0);
    let orphan_log = log.clone();
    effect(move || {
        orphan.get();
        bump();
        let log = orphan_log.clone();
        on_cleanup(move || log.borrow_mut().push("effect"));
    });
    let scope_log = log.clone();
    Scope::root().run(move || on_cleanup(move || scope_log.borrow_mut().push("scope")));
    drop(entered);
    assert!(log.borrow().is_empty());

    drop(runtime);
    let mut seen = log.borrow().clone();
    seen.sort_unstable();
    assert_eq!(seen, ["effect", "scope"]);
    assert_eq!(runs.get(), 1);
    assert!(!orphan.is_alive());
}

#[test]
fn cleanups_can_use_the_runtime_while_it_is_dropped() {
    let seen = Rc::new(RefCell::new(None));
    let runtime = Runtime::new();
    {
        let _entered = runtime.enter();
        let s = signal(7);
        let seen = seen.clone();
        Scope::root().run(move || on_cleanup(move || *seen.borrow_mut() = s.try_get()));
    }
    drop(runtime);
    assert!(seen.borrow().is_some_and(|v| v == 7));
}

#[test]
fn runtimes_are_isolated_and_nest() {
    let outer = Runtime::new();
    let _outer = outer.enter();
    let a = signal(1);
    let (runs, bump) = counter();
    effect(move || {
        a.get();
        bump();
    });
    {
        let inner = Runtime::new();
        let _inner = inner.enter();
        assert!(
            !a.is_alive(),
            "a handle is not alive in a runtime that did not create it"
        );
        let b = signal(10);
        assert_eq!(b.get(), 10);
    }
    a.set(2);
    assert_eq!(a.get(), 2);
    assert_eq!(runs.get(), 2);
}

#[test]
fn an_entered_guard_keeps_its_runtime_alive_until_it_is_dropped() {
    let (cleanups, bump) = counter();
    let entered = Runtime::new().enter();
    let s = signal(1);
    Scope::root().run(move || on_cleanup(bump));
    assert_eq!(s.get(), 1);
    assert_eq!(cleanups.get(), 0);
    drop(entered);
    assert_eq!(cleanups.get(), 1);
}
