use rouge_reactive::testing::in_root;
use rouge_reactive::{ReactiveStats, Runtime, Scope, effect, memo, signal, stats};

#[test]
fn effect_and_memo_runs_and_created_nodes_are_counted() {
    in_root(|| {
        let before = stats();
        let source = signal(1);
        let doubled = memo(move || source.get() * 2);
        effect(move || {
            doubled.get();
        });
        source.set(2);
        source.set(3);
        let delta = stats() - before;
        assert_eq!((delta.memos_run, delta.effects_run), (3, 3));
        assert_eq!(delta.nodes_created, 3);
    });
}

#[test]
fn disposing_every_subscriber_of_a_signal_unsubscribes_them_all() {
    in_root(|| {
        let shared = signal(0);
        let scope = Scope::new();
        scope.run(|| {
            for _ in 0..1_000 {
                effect(move || {
                    shared.get();
                });
            }
        });
        let before = stats();
        scope.dispose();
        shared.set(1);
        let delta = stats() - before;
        assert_eq!(delta.nodes_disposed, 1_001);
        assert_eq!(delta.effects_run, 0);
    });
}

#[test]
fn rerunning_with_the_same_dependencies_keeps_one_subscription_each() {
    in_root(|| {
        let shared = signal(0);
        let trigger = signal(0);
        effect(move || {
            shared.get();
            trigger.get();
        });
        trigger.set(1);
        trigger.set(2);
        let before = stats();
        shared.set(1);
        assert_eq!((stats() - before).effects_run, 1);
    });
}

#[test]
fn stats_are_zero_without_an_entered_runtime() {
    let _runtime = Runtime::new();
    assert_eq!(stats(), ReactiveStats::default());
}
