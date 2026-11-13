use rouge_reactive::testing::{counter, in_root, recorder};
use rouge_reactive::{
    Runtime, Scope, batch, effect, expect_context, memo, on_cleanup, provide_context, run_in,
    signal, untrack, use_context,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[test]
fn batch_coalesces_writes() {
    in_root(|| {
        let a = signal(1);
        let b = signal(2);
        let (seen, record) = recorder();
        effect(move || record(a.get() + b.get()));
        batch(|| {
            a.set(10);
            batch(|| b.set(20));
            assert_eq!(seen.borrow().len(), 1, "no effect runs inside a batch");
            a.set(100);
        });
        assert_eq!(*seen.borrow(), vec![3, 120]);
    });
}

#[test]
fn untrack_does_not_subscribe() {
    in_root(|| {
        let a = signal(1);
        let b = signal(1);
        let (runs, bump) = counter();
        effect(move || {
            a.get();
            untrack(|| b.get());
            b.get_untracked();
            bump();
        });
        b.set(2);
        assert_eq!(runs.get(), 1);
        a.set(2);
        assert_eq!(runs.get(), 2);
    });
}

#[test]
fn scope_disposal_frees_nodes_and_stops_effects() {
    in_root(|| {
        let outer = signal(0);
        let (runs, bump) = counter();
        let child = Scope::new();
        let (inner, m, e) = child.run(|| {
            let inner = signal(1);
            let m = memo(move || outer.get() + inner.get());
            let e = effect(move || {
                m.get();
                bump();
            });
            (inner, m, e)
        });
        assert_eq!(runs.get(), 1);
        outer.set(1);
        assert_eq!(runs.get(), 2);

        child.dispose();
        child.dispose();
        assert!(!child.is_alive() && !inner.is_alive() && !m.is_alive() && !e.is_alive());
        assert!(outer.is_alive());

        outer.set(2);
        assert_eq!(runs.get(), 2, "disposed effect must not run");
    });
}

#[test]
fn child_scopes_dispose_in_reverse_order() {
    let _entered = Runtime::new().enter();
    let log = Rc::new(RefCell::new(Vec::new()));
    let root = Scope::root();
    root.run(|| {
        for name in ["a", "b"] {
            let log = log.clone();
            Scope::new().run(move || on_cleanup(move || log.borrow_mut().push(name)));
        }
        let log = log.clone();
        on_cleanup(move || log.borrow_mut().push("root"));
    });
    root.dispose();
    assert_eq!(*log.borrow(), ["b", "a", "root"]);
}

#[test]
fn scope_run_does_not_track_into_enclosing_effect() {
    in_root(|| {
        let s = signal(0);
        let (runs, bump) = counter();
        effect(move || {
            bump();
            Scope::new().run(|| {
                s.get();
            });
        });
        s.set(1);
        assert_eq!(runs.get(), 1);
    });
}

#[test]
fn context_lookup_walks_owners_and_shadows() {
    in_root(|| {
        provide_context(1u32);
        provide_context("theme");
        assert_eq!(use_context::<u32>(), Some(1));
        assert_eq!(use_context::<u64>(), None);

        let child = Scope::new();
        child.run(|| {
            assert_eq!(use_context::<u32>(), Some(1));
            provide_context(2u32);
            assert_eq!(expect_context::<u32>(), 2);
            let seen = Rc::new(Cell::new(0));
            let seen2 = seen.clone();
            effect(move || seen2.set(expect_context::<u32>()));
            assert_eq!(seen.get(), 2, "effects see context from their owner");
            assert_eq!(use_context::<&str>(), Some("theme"));
        });
        assert_eq!(use_context::<u32>(), Some(1));
    });
}

#[test]
fn current_scope_runs_later_work_as_its_owner_until_disposed() -> Result<(), &'static str> {
    let _entered = Runtime::new().enter();
    let scope = Scope::root();
    let current = scope.run(Scope::current).ok_or("a scope is current")?;
    assert_eq!(current, scope);
    let value = run_in(Some(current), || signal(1)).ok_or("the scope is alive")?;
    scope.dispose();
    assert!(!value.is_alive());
    assert_eq!(scope.try_run(|| 1), None);
    assert_eq!(run_in(Some(scope), || 1), None);
    assert_eq!(run_in(None, || 1), Some(1));
    Ok(())
}
