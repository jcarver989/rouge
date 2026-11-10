use rouge_reactive::testing::{counter, in_root, recorder};
use rouge_reactive::{batch, effect, memo, signal};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[test]
fn memo_is_lazy_and_cached() {
    in_root(|| {
        let s = signal(2);
        let (runs, bump) = counter();
        let m = memo(move || {
            bump();
            s.get() * 2
        });
        assert_eq!(runs.get(), 0, "memo must not run before first read");
        assert_eq!(m.get(), 4);
        assert_eq!(m.get(), 4);
        assert_eq!(runs.get(), 1);
        s.set(3);
        assert_eq!(runs.get(), 1, "unobserved memo must not recompute eagerly");
        assert_eq!(m.get(), 6);
        assert_eq!(runs.get(), 2);
    });
}

#[test]
fn memo_short_circuits_when_value_unchanged() {
    in_root(|| {
        let s = signal(1);
        let parity = memo(move || s.get() % 2);
        let (runs, bump) = counter();
        effect(move || {
            parity.get();
            bump();
        });
        assert_eq!(runs.get(), 1);
        s.set(3);
        s.set(5);
        assert_eq!(
            runs.get(),
            1,
            "effect must not re-run when memo value is equal"
        );
        s.set(4);
        assert_eq!(runs.get(), 2);
    });
}

#[test]
fn diamond_is_glitch_free_and_runs_once() {
    in_root(|| {
        let a = signal(1);
        let b = memo(move || a.get() + 1);
        let c = memo(move || a.get() * 10);
        let (seen, record) = recorder();
        effect(move || record((b.get(), c.get())));
        a.set(2);
        a.set(3);
        assert_eq!(*seen.borrow(), vec![(2, 10), (3, 20), (4, 30)]);
    });
}

#[test]
fn deep_chain_propagates() {
    in_root(|| {
        let s = signal(0u64);
        let mut m = memo(move || s.get());
        for _ in 0..200 {
            let prev = m;
            m = memo(move || prev.get() + 1);
        }
        let out = Rc::new(Cell::new(0));
        let out2 = out.clone();
        effect(move || out2.set(m.get()));
        assert_eq!(out.get(), 200);
        s.set(5);
        assert_eq!(out.get(), 205);
    });
}

#[test]
fn reads_inside_batch_see_fresh_memos() {
    in_root(|| {
        let a = signal(1);
        let double = memo(move || a.get() * 2);
        batch(|| {
            a.set(5);
            assert_eq!(double.get(), 10);
        });
    });
}

#[test]
fn memo_owned_nodes_are_disposed_on_recompute() {
    in_root(|| {
        let s = signal(0);
        let owned = Rc::new(RefCell::new(Vec::new()));
        let owned2 = owned.clone();
        let m = memo(move || {
            owned2.borrow_mut().push(signal(s.get()));
            s.get()
        });
        for i in 0..10 {
            s.set(i);
            m.get();
        }
        let alive = owned.borrow().iter().filter(|o| o.is_alive()).count();
        assert_eq!(
            (owned.borrow().len(), alive),
            (10, 1),
            "only the latest run's signal survives"
        );
    });
}
