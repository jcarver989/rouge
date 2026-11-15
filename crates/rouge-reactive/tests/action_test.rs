use rouge_reactive::testing::{Driver, recorder};
use rouge_reactive::{action, effect, poll_tasks};

#[test]
fn an_action_is_pending_until_its_future_resolves() {
    let _driver = Driver::new();
    let double = action(|n: u32| async move { n * 2 });
    let (seen, record) = recorder();
    effect(move || record((double.pending(), double.value())));
    double.dispatch(21);
    poll_tasks();
    assert_eq!(
        *seen.borrow(),
        [(false, None), (true, None), (false, Some(42))]
    );
}

#[test]
fn only_the_latest_dispatch_settles_the_action() {
    let _driver = Driver::new();
    let echo = action(|n: u32| async move { n });
    echo.dispatch(1);
    echo.dispatch(2);
    poll_tasks();
    assert_eq!((echo.pending(), echo.value()), (false, Some(2)));
}
