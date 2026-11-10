use rouge_reactive::testing::{Driver, TestResult, recorder};
use rouge_reactive::{Scope, effect, poll_tasks, signal};
use std::thread;

#[test]
fn set_is_applied_on_the_next_poll() {
    let driver = Driver::new();
    let s = signal(0);
    let (seen, record) = recorder();
    effect(move || record(s.get()));
    s.sender().set(3);
    assert_eq!(s.get(), 0, "the write is posted, not applied in place");
    assert_eq!(driver.wakes(), 1, "posting asks the event loop for a poll");
    assert!(poll_tasks());
    assert_eq!(*seen.borrow(), [0, 3]);
    assert!(!poll_tasks());
}

#[test]
fn senders_work_from_other_threads() -> TestResult {
    let driver = Driver::new();
    let s = signal(vec![0]);
    let sender = s.sender();
    for i in 1..=3 {
        let sender = sender.clone();
        thread::spawn(move || sender.update(move |v| v.push(i)));
    }
    driver.wait_for_wake()?;
    driver.run_until(|| s.with(Vec::len) == 4)?;
    let mut values = s.get();
    values.sort_unstable();
    assert_eq!(values, [0, 1, 2, 3]);
    Ok(())
}

#[test]
fn writes_to_a_disposed_signal_are_ignored() {
    let _driver = Driver::new();
    let scope = Scope::root();
    let s = scope.run(|| signal(0));
    let sender = s.sender();
    scope.dispose();
    sender.set(1);
    sender.update(|v| *v += 1);
    assert!(poll_tasks());
    assert!(!s.is_alive());
}

#[test]
fn sending_after_the_runtime_is_gone_is_a_no_op() {
    let driver = Driver::new();
    let sender = { signal(0).sender() };
    drop(driver);
    sender.set(1);
}
