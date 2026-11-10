use rouge_reactive::{Runtime, Scope, signal};

#[test]
fn disposed_signal_reports_through_try_variants() {
    let _entered = Runtime::new().enter();
    let scope = Scope::root();
    let s = scope.run(|| signal(1));
    scope.dispose();
    assert!(!s.is_alive());
    assert_eq!(s.try_get(), None);
    assert_eq!(s.try_set(5), Err(5));
    assert_eq!(s.try_update(|v| *v), None);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "already disposed")]
fn reading_disposed_signal_panics() {
    let _entered = Runtime::new().enter();
    let scope = Scope::root();
    let s = scope.run(|| signal(1));
    scope.dispose();
    s.get();
}
