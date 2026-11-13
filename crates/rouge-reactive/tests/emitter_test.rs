use rouge_reactive::testing::{in_root, recorder};
use rouge_reactive::{Scope, emitter};

#[test]
fn subscribers_hear_each_event_until_their_scope_is_disposed() {
    in_root(|| {
        let events = emitter::<u32>();
        let (kept, keep) = recorder();
        let (dropped, drop) = recorder();
        events.subscribe(keep);
        let scope = Scope::new();
        scope.run(|| events.subscribe(drop));
        events.emit(1);
        scope.dispose();
        events.emit(2);
        assert_eq!(*kept.borrow(), [1, 2]);
        assert_eq!(*dropped.borrow(), [1]);
    });
}
