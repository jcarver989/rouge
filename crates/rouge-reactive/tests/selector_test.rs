use rouge_reactive::testing::{Driver, counter};
use rouge_reactive::{Scope, effect, selector, signal, stats};

#[test]
fn a_selection_change_notifies_only_the_old_and_new_keys() {
    let _driver = Driver::new();
    let selected = signal(Some(0usize));
    let selection = selector(move || selected.get());
    let runs = Vec::from_fn(1000, |row| {
        let (runs, bump) = counter();
        let on = selection.is(Some(row));
        effect(move || {
            on.get();
            bump();
        });
        runs
    });
    assert!(selection.is(Some(0)).get_untracked());
    let before = stats();
    selected.set(Some(500));
    let work = stats() - before;
    assert_eq!(work.effects_run, 3, "the source effect and two rows");
    assert_eq!(runs[0].get(), 2);
    assert_eq!(runs[500].get(), 2);
    assert_eq!(runs[1].get(), 1);
    assert!(selection.is(Some(500)).get_untracked());
    assert!(!selection.is(Some(0)).get_untracked());
}

#[test]
fn a_key_is_forgotten_when_its_last_reader_is_disposed() {
    let _driver = Driver::new();
    let selected = signal(1);
    let selection = selector(move || selected.get());
    let row = Scope::new();
    let on = row.run(|| selection.is(2));
    assert!(on.is_alive());
    row.dispose();
    assert!(
        !on.is_alive(),
        "the per-key signal goes with its last reader"
    );
    selected.set(2);
    assert!(selection.is(2).get_untracked());
}
