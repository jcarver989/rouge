use rouge_reactive::testing::{counter, in_root, recorder};
use rouge_reactive::{Runtime, Scope, effect, on_cleanup, signal};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[test]
fn effect_runs_immediately_and_on_change() {
    in_root(|| {
        let s = signal(0);
        let (seen, record) = recorder();
        effect(move || record(s.get()));
        s.set(1);
        s.set(2);
        assert_eq!(*seen.borrow(), vec![0, 1, 2]);
    });
}

#[test]
fn dependencies_are_dynamic() {
    in_root(|| {
        let use_a = signal(true);
        let a = signal(1);
        let b = signal(100);
        let (runs, bump) = counter();
        let last = Rc::new(Cell::new(0));
        let last2 = last.clone();
        effect(move || {
            bump();
            last2.set(if use_a.get() { a.get() } else { b.get() });
        });
        b.set(101);
        assert_eq!(runs.get(), 1, "b is not a dependency yet");
        use_a.set(false);
        assert_eq!((runs.get(), last.get()), (2, 101));
        a.set(2);
        assert_eq!(runs.get(), 2, "a is no longer a dependency");
        b.set(102);
        assert_eq!((runs.get(), last.get()), (3, 102));
    });
}

#[test]
fn effect_writing_other_signal_cascades() {
    in_root(|| {
        let a = signal(1);
        let b = signal(0);
        let (seen, record) = recorder();
        effect(move || b.set(a.get() * 2));
        effect(move || record(b.get()));
        a.set(5);
        assert_eq!(*seen.borrow(), vec![2, 10]);
    });
}

#[test]
fn self_writing_effect_settles() {
    in_root(|| {
        let n = signal(0);
        let (runs, bump) = counter();
        effect(move || {
            bump();
            if n.get() < 5 {
                n.update(|v| *v += 1);
            }
        });
        assert_eq!(n.get_untracked(), 5);
        assert_eq!(runs.get(), 6);
    });
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "did not settle")]
fn effect_cycle_is_reported() {
    let _entered = Runtime::new().enter();
    let root = Scope::root();
    root.run(|| {
        let n = signal(0u64);
        effect(move || n.set(n.get() + 1));
    });
}

#[test]
fn nested_effects_are_disposed_on_rerun() {
    in_root(|| {
        let outer = signal(0);
        let inner = signal(0);
        let (inner_runs, bump) = counter();
        effect(move || {
            outer.get();
            let bump = bump.clone();
            effect(move || {
                inner.get();
                bump();
            });
        });
        assert_eq!(inner_runs.get(), 1);
        outer.set(1);
        outer.set(2);
        assert_eq!(inner_runs.get(), 3, "one fresh inner effect per outer run");
        inner.set(1);
        assert_eq!(inner_runs.get(), 4, "only the latest inner effect is alive");
    });
}

#[test]
fn cleanup_runs_before_rerun_and_on_dispose() {
    let _entered = Runtime::new().enter();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let root = Scope::root();
    let s = root.run(|| {
        let s = signal(0);
        let log = log.clone();
        effect(move || {
            let v = s.get();
            log.borrow_mut().push(format!("run {v}"));
            let log = log.clone();
            on_cleanup(move || log.borrow_mut().push(format!("cleanup {v}")));
        });
        s
    });
    s.set(1);
    root.dispose();
    assert_eq!(*log.borrow(), ["run 0", "cleanup 0", "run 1", "cleanup 1"]);
}

#[test]
fn effect_dispose_stops_it() {
    in_root(|| {
        let s = signal(0);
        let (runs, bump) = counter();
        let e = effect(move || {
            s.get();
            bump();
        });
        e.dispose();
        assert!(!e.is_alive());
        s.set(1);
        assert_eq!(runs.get(), 1);
    });
}
