use rouge_reactive::testing::{Driver, TestResult};
use rouge_reactive::{Scope, interval};
use std::time::Duration;

#[test]
fn interval_ticks_until_its_scope_is_disposed() -> TestResult {
    let driver = Driver::new();
    let scope = Scope::root();
    let ticks = scope.run(|| interval(Duration::from_millis(1)));
    driver.run_until(|| ticks.get() >= 2)?;
    scope.dispose();
    assert!(!ticks.is_alive());
    Ok(())
}
