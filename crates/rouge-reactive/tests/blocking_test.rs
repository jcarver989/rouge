use rouge_reactive::testing::{Driver, TestResult};
use rouge_reactive::{background, signal, sleep, spawn};
use std::thread;
use std::time::Duration;

#[test]
fn background_returns_blocking_work_to_the_ui_thread() -> TestResult {
    let driver = Driver::new();
    let s = signal(None);
    let ui_thread = thread::current().id();
    spawn(async move {
        let worker = background(async { thread::current().id() }).await;
        s.set(Some(worker));
    });
    driver.run_until(|| s.get().is_some())?;
    assert!(s.get().is_some_and(|worker| worker != ui_thread));
    Ok(())
}

#[test]
fn background_runs_a_future_to_completion_off_the_ui_thread() -> TestResult {
    let driver = Driver::new();
    let s = signal(None);
    let ui_thread = thread::current().id();
    spawn(async move {
        let result = background(async {
            sleep(Duration::from_millis(2)).await;
            let inner = background(async { 20 }).await;
            (inner + 1, thread::current().id())
        })
        .await;
        s.set(Some(result));
    });
    driver.run_until(|| s.get().is_some())?;
    assert!(
        s.get()
            .is_some_and(|(value, worker)| value == 21 && worker != ui_thread)
    );
    Ok(())
}

#[test]
#[should_panic(expected = "thread panicked before producing its result")]
fn awaiting_a_panicked_thread_panics_with_a_clear_message() {
    let driver = Driver::new();
    let s = signal(false);
    spawn(async move {
        background(async { panic!("worker failed") }).await;
        s.set(true);
    });
    let _ = driver.run_until(|| s.get());
}
