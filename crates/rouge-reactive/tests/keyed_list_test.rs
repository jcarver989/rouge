use rouge_reactive::testing::{Driver, counter};
use rouge_reactive::{KeyedList, effect, keyed_list, memo, stats};

#[derive(Clone, Debug, PartialEq)]
struct Item {
    id: u32,
    done: bool,
}

#[test]
fn values_keep_their_order_through_pushes_and_removals() {
    let _driver = Driver::new();
    let items = list(5);
    assert_eq!(items.remove(&2), Some(Item { id: 2, done: false }));
    items.push(Item { id: 9, done: true });
    items.push(Item { id: 0, done: true });
    items.retain(|item| item.id != 3);
    assert_eq!(items.keys().get_untracked(), vec![0, 1, 4, 9]);
    assert_eq!(items.with(&2, |_| ()), None);
    let done = items.with_values(|all| all.iter().map(|item| item.done).collect::<Vec<_>>());
    assert_eq!(done, vec![true, false, false, true]);
    assert!(items.move_to(&9, 0) && !items.move_to(&2, 0));
    assert_eq!(items.keys().get_untracked(), vec![9, 0, 1, 4]);
}

#[test]
fn updating_one_entry_runs_only_that_entrys_readers() {
    let _driver = Driver::new();
    let items = list(1000);
    let runs: Vec<_> = (0..1000)
        .map(|id| {
            let (runs, bump) = counter();
            let done = items.select(id, |item| item.done);
            effect(move || {
                done.get();
                bump();
            });
            runs
        })
        .collect();
    let before = stats();
    items.update(&500, |item| item.done = true);
    let work = stats() - before;
    assert_eq!(work.memos_run, 1);
    assert_eq!(work.effects_run, 1);
    assert_eq!(runs[500].get(), 2);
    assert_eq!(runs[499].get(), 1);
}

#[test]
fn derived_values_follow_every_write() {
    let _driver = Driver::new();
    let items = list(6);
    let done = memo(move || items.with_values(|all| all.iter().filter(|item| item.done).count()));
    items.update(&1, |item| item.done = true);
    assert_eq!(done.get_untracked(), 1);
    items.update_all(|item| item.done = item.id % 2 == 0);
    assert_eq!(done.get_untracked(), 3);
    items.retain(|item| !item.done);
    assert_eq!(done.get_untracked(), 0);
    assert_eq!(items.keys().get_untracked(), vec![1, 3, 5]);
}

#[test]
fn a_selection_of_a_removed_key_keeps_its_last_value() {
    let _driver = Driver::new();
    let items = list(2);
    let done = items.select(1, |item| item.done);
    items.update(&1, |item| item.done = true);
    assert!(done.get_untracked());
    items.remove(&1);
    assert!(done.get_untracked());
}

fn list(count: u32) -> KeyedList<u32, Item> {
    let items = keyed_list(|item: &Item| item.id);
    items.extend((0..count).map(|id| Item { id, done: false }));
    items
}
