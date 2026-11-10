use rouge_reactive::testing::{counter, in_root};
use rouge_reactive::{effect, signal};

#[test]
fn signal_get_set_update() {
    in_root(|| {
        let s = signal(1);
        assert_eq!(s.get(), 1);
        s.set(2);
        assert_eq!(s.get(), 2);
        s.update(|v| *v += 10);
        assert_eq!(s.with(|v| *v), 12);
        assert_eq!(s.try_update(|v| *v * 2), Some(24));
        let read = s.read_only();
        s.set(3);
        assert_eq!(read.get(), 3, "a read-only handle shares state");
    });
}

#[test]
fn set_if_changed_skips_equal_values() {
    in_root(|| {
        let s = signal(1);
        let (runs, bump) = counter();
        effect(move || {
            s.get();
            bump();
        });
        assert!(!s.set_if_changed(1));
        assert_eq!(runs.get(), 1);
        assert!(s.set_if_changed(2));
        assert_eq!(runs.get(), 2);
    });
}

#[test]
fn many_subscribers_all_notified_once() {
    in_root(|| {
        let s = signal(0);
        let (runs, bump) = counter();
        for _ in 0..1000 {
            let bump = bump.clone();
            effect(move || {
                s.get();
                bump();
            });
        }
        s.set(1);
        assert_eq!(runs.get(), 2000);
    });
}
