use rouge_reactive::testing::{Driver, TestResult, recorder};
use rouge_reactive::{Runtime, Scope, Task, effect, on_cleanup, poll_tasks, signal, spawn};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::thread;
use tokio::sync::oneshot::channel;

#[test]
fn spawned_task_runs_on_poll_and_reruns_effects() {
    let _driver = Driver::new();
    let s = signal(0);
    let (seen, record) = recorder();
    effect(move || record(s.get()));
    let task = spawn(async move {
        s.set(1);
        s.set(2);
    });
    assert!(!task.is_finished(), "spawn does not poll synchronously");
    assert_eq!(*seen.borrow(), [0]);
    assert!(poll_tasks());
    assert_eq!(*seen.borrow(), [0, 2], "writes within one poll are batched");
    assert!(task.is_finished());
    assert!(!poll_tasks(), "nothing is left to run");
}

#[test]
fn task_resumes_across_awaits() -> TestResult {
    let _driver = Driver::new();
    let s = signal(0);
    let (tx, rx) = channel();
    let task = spawn(async move {
        s.set(1);
        if let Ok(v) = rx.await {
            s.set(v);
        }
    });
    assert!(poll_tasks());
    assert_eq!(s.get(), 1);
    assert!(!task.is_finished());
    assert!(
        !poll_tasks(),
        "a pending task is not polled until it is woken"
    );
    tx.send(5).map_err(|_| "the task dropped its receiver")?;
    assert!(poll_tasks());
    assert_eq!(s.get(), 5);
    assert!(task.is_finished());
    Ok(())
}

#[test]
fn task_runs_in_the_scope_that_spawned_it() -> TestResult {
    let _driver = Driver::new();
    let scope = Scope::root();
    let created = Rc::new(Cell::new(None));
    let cleaned = Rc::new(Cell::new(false));
    let (created2, cleaned2) = (created.clone(), cleaned.clone());
    scope.run(move || {
        spawn(async move {
            created2.set(Some(signal(1)));
            on_cleanup(move || cleaned2.set(true));
        })
    });
    poll_tasks();
    let inner = created.get().ok_or("task did not run")?;
    assert!(inner.is_alive());
    assert!(!cleaned.get());
    scope.dispose();
    assert!(
        !inner.is_alive(),
        "signals created in a task belong to the spawning scope"
    );
    assert!(cleaned.get());
    Ok(())
}

#[test]
fn disposing_the_scope_cancels_its_tasks() {
    let _driver = Driver::new();
    let scope = Scope::root();
    let resumed = Rc::new(Cell::new(false));
    let resumed2 = resumed.clone();
    let (tx, rx) = channel::<()>();
    let task = scope.run(move || {
        spawn(async move {
            let _ = rx.await;
            resumed2.set(true);
        })
    });
    poll_tasks();
    assert!(!task.is_finished());
    scope.dispose();
    assert!(task.is_finished());
    assert!(tx.send(()).is_err(), "the cancelled future was dropped");
    poll_tasks();
    assert!(!resumed.get());
}

#[test]
#[expect(clippy::no_effect_underscore_binding)]
fn cancel_drops_the_future() {
    let _driver = Driver::new();
    let ran = Rc::new(Cell::new(false));
    let ran2 = ran.clone();
    let alive = Rc::new(());
    let held = alive.clone();
    let task = spawn(async move {
        let _held = held;
        ran2.set(true);
    });
    task.cancel();
    assert!(task.is_finished());
    assert_eq!(Rc::strong_count(&alive), 1);
    assert!(!poll_tasks());
    assert!(!ran.get());
}

#[test]
fn task_can_cancel_itself_while_running() {
    let _driver = Driver::new();
    let handle = Rc::new(RefCell::new(None::<Task>));
    let handle2 = handle.clone();
    let (tx, rx) = channel::<()>();
    let task = spawn(async move {
        if let Some(task) = &*handle2.borrow() {
            task.cancel();
        }
        let _ = rx.await;
    });
    *handle.borrow_mut() = Some(task);
    poll_tasks();
    assert!(handle.borrow().as_ref().is_some_and(Task::is_finished));
    assert!(tx.send(()).is_err());
}

#[test]
#[expect(clippy::no_effect_underscore_binding)]
fn dropping_the_runtime_drops_pending_tasks() {
    let alive = Rc::new(());
    let held = alive.clone();
    let runtime = Runtime::new();
    let (tx, rx) = channel::<()>();
    {
        let _entered = runtime.enter();
        spawn(async move {
            let _held = held;
            let _ = rx.await;
        });
        poll_tasks();
    }
    assert_eq!(Rc::strong_count(&alive), 2);
    drop(runtime);
    assert_eq!(Rc::strong_count(&alive), 1);
    assert!(tx.send(()).is_err());
}

#[test]
fn waker_hook_fires_when_a_task_is_woken_from_another_thread() -> TestResult {
    let driver = Driver::new();
    let s = signal(0);
    let (tx, rx) = channel();
    spawn(async move {
        if let Ok(v) = rx.await {
            s.set(v);
        }
    });
    poll_tasks();
    assert_eq!(
        driver.wakes(),
        1,
        "only the spawn has asked for a poll so far"
    );
    thread::spawn(move || tx.send(7));
    driver.wait_for_wake()?;
    assert!(poll_tasks());
    assert_eq!(s.get(), 7);
    Ok(())
}
