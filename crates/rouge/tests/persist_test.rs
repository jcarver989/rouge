#![cfg(all(feature = "persist", not(target_arch = "wasm32")))]

use rouge::core::testing::{TestResult, TestUi, TestUiBuilder};
use rouge::persist::{FileStorage, MemoryStorage, Storage};
use rouge::prelude::*;
use std::fs::read_to_string;
use std::rc::Rc;

#[test]
fn a_persisted_signal_is_saved_when_its_window_closes_and_restored_on_the_next() {
    let _entered = Runtime::new().enter();
    let storage = MemoryStorage::default();
    let (ui, count) = window(storage.clone(), 1);
    count.set(7);
    drop(ui);
    assert_eq!(storage.get("count").as_deref(), Some("7"));
    let (_ui, restored) = window(storage, 0);
    assert_eq!(restored.get_untracked(), 7);
}

#[test]
fn unreadable_saved_state_is_ignored() {
    let _entered = Runtime::new().enter();
    let storage = MemoryStorage::default();
    storage.insert("count", "not json");
    let (_ui, count) = window(storage, 3);
    assert_eq!(count.get_untracked(), 3);
}

#[test]
fn file_storage_keeps_each_key_in_its_own_file() -> TestResult {
    let _entered = Runtime::new().enter();
    let dir = tempfile::tempdir()?;
    let storage = FileStorage::new(dir.path().join("nested"));
    drop(storage.write("count", "7".to_owned()));
    drop(storage);
    assert_eq!(read_to_string(dir.path().join("nested/count.json"))?, "7");
    let (mut ui, restored) = window(FileStorage::new(dir.path().join("nested")), 0);
    assert!(ui.wait_until(|_| restored.get_untracked() == 7));
    Ok(())
}

fn window(storage: impl Storage + 'static, initial: u32) -> (TestUi, Signal<u32>) {
    let (mut ui, count) = TestUiBuilder::default().build_with(move || {
        provide_storage(Rc::new(storage));
        (div(), persisted("count", signal(initial)))
    });
    ui.advance(0.0);
    (ui, count)
}
