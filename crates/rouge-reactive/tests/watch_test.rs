use rouge_reactive::testing::{in_root, recorder};
use rouge_reactive::{Scope, signal, spawn, watch};

#[test]
fn watch_calls_the_handler_with_each_distinct_value_and_the_previous_one() {
    in_root(|| {
        let count = signal(1);
        let (seen, record) = recorder();
        watch(
            move || count.get() / 2,
            move |value, previous| record((*value, previous.copied())),
        );
        count.set(2);
        count.set(3);
        count.set(4);
        assert_eq!(*seen.borrow(), [(0, None), (1, Some(0)), (2, Some(1))]);
    });
}

#[test]
fn the_handler_is_untracked_and_owned_by_the_enclosing_scope() {
    in_root(|| {
        let source = signal(0);
        let other = signal(0);
        let scope = Scope::current();
        let (seen, record) = recorder();
        let (owners, own) = recorder();
        watch(
            move || source.get(),
            move |value, _| {
                record(*value + other.get());
                own(Scope::current());
                spawn(async {});
            },
        );
        other.set(5);
        source.set(1);
        assert_eq!(*seen.borrow(), [0, 6]);
        assert!(owners.borrow().iter().all(|owner| *owner == scope));
    });
}
