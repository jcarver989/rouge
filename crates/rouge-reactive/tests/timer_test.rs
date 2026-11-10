use rouge_reactive::testing::{Driver, TestResult};
use rouge_reactive::{poll_tasks, signal, sleep, spawn};
use std::time::{Duration, Instant};

#[test]
fn sleeps_add_up_and_complete_in_deadline_order() -> TestResult {
    let driver = Driver::new();
    let log = signal(Vec::new());
    let started = Instant::now();
    for ms in [30, 5, 15] {
        spawn(async move {
            sleep(Duration::from_millis(ms / 2)).await;
            sleep(Duration::from_millis(ms - ms / 2)).await;
            log.update(|l| l.push(ms));
        });
    }
    driver.run_until(|| log.with(Vec::len) == 3)?;
    assert_eq!(log.get(), [5, 15, 30]);
    assert!(
        started.elapsed() >= Duration::from_millis(30),
        "sequential sleeps add up"
    );
    Ok(())
}

#[test]
fn zero_sleep_is_ready_on_first_poll() {
    let _driver = Driver::new();
    let done = signal(false);
    spawn(async move {
        sleep(Duration::ZERO).await;
        done.set(true);
    });
    poll_tasks();
    assert!(done.get());
}
