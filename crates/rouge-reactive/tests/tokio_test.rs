#![cfg(feature = "tokio-rt")]

use rouge_reactive::testing::{Driver, TestResult};
use rouge_reactive::{Scope, TaskError, set_tokio, signal, spawn, spawn_blocking, spawn_tokio};
use std::future::pending;
use std::io::Error as IoError;
use std::mem::replace;
use std::thread;
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::oneshot::{Sender, channel};
use tokio::time::sleep;

#[test]
fn tokio_io_and_completion_cross_back_to_the_ui_executor() -> TestResult {
    let driver = Driver::new();
    let runtime = tokio_runtime()?;
    set_tokio(runtime.handle().clone());
    let owner = thread::current().id();
    let completed = signal(None);
    let updates = signal(0);
    let sender = updates.sender();
    let task = spawn_tokio(async move {
        sleep(Duration::from_millis(1)).await;
        sender.set(4);
        7usize
    });
    spawn(async move {
        completed.set(Some(
            task.await.map(|value| (thread::current().id(), value)),
        ));
    });
    driver.run_until(|| completed.with(Option::is_some))?;
    assert_eq!(
        completed
            .try_update(Option::take)
            .flatten()
            .ok_or("missing task result")??,
        (owner, 7)
    );
    assert_eq!(updates.get(), 4);
    Ok(())
}

#[test]
fn cancelling_on_drop_or_disposing_the_scope_aborts_tokio_and_drops_the_future() -> TestResult {
    let driver = Driver::new();
    let runtime = tokio_runtime()?;
    set_tokio(runtime.handle().clone());
    for dispose_scope in [false, true] {
        let scope = Scope::root();
        let (started, ready) = channel();
        let (dropped, closed) = channel();
        let task = scope.run(|| {
            spawn_tokio(async move {
                let _drop = DropNotice(dropped);
                let _ = started.send(());
                pending::<()>().await;
            })
            .cancel_on_drop()
        });
        runtime.block_on(ready)?;
        if dispose_scope {
            scope.dispose();
        } else {
            drop(task);
        }
        runtime.block_on(closed)?;
        driver.wakes();
        if scope.is_alive() {
            scope.dispose();
        }
    }
    Ok(())
}

#[test]
fn completion_is_published_only_on_ui_poll_and_tokio_panics_are_reported() -> TestResult {
    let driver = Driver::new();
    let runtime = tokio_runtime()?;
    set_tokio(runtime.handle().clone());
    let (ran, ready) = channel();
    let task = spawn_tokio(async move {
        let _ = ran.send(());
        42
    });
    runtime.block_on(ready)?;
    assert!(!task.is_finished());
    let result = signal(None);
    spawn(async move {
        result.set(Some(task.await));
    });
    driver.run_until(|| result.with(Option::is_some))?;
    assert!(result.with(|result| matches!(result, Some(Ok(42)))));
    let failed = signal(false);
    let task = spawn_tokio(async { panic!("intentional task failure") });
    spawn(async move {
        failed.set(matches!(task.await, Err(TaskError::Tokio(_))));
    });
    driver.run_until(|| failed.get())?;
    Ok(())
}

#[test]
fn a_dropped_task_keeps_running_in_its_scope() -> TestResult {
    let driver = Driver::new();
    let runtime = tokio_runtime()?;
    set_tokio(runtime.handle().clone());
    let done = signal(false);
    let sender = done.sender();
    drop(spawn_tokio(async move { sender.set(true) }));
    driver.run_until(|| done.get())?;
    Ok(())
}

#[test]
fn blocking_work_runs_on_the_tokio_pool_and_returns_to_the_ui() -> TestResult {
    let driver = Driver::new();
    let runtime = tokio_runtime()?;
    set_tokio(runtime.handle().clone());
    let owner = thread::current().id();
    let result = signal(None);
    let task = spawn_blocking(move || thread::current().id() != owner);
    spawn(async move { result.set(Some(task.await)) });
    driver.run_until(|| result.with(Option::is_some))?;
    assert!(result.with(|result| matches!(result, Some(Ok(true)))));
    Ok(())
}

fn tokio_runtime() -> Result<Runtime, IoError> {
    Builder::new_multi_thread()
        .worker_threads(1)
        .enable_time()
        .build()
}

struct DropNotice(Sender<()>);

impl Drop for DropNotice {
    fn drop(&mut self) {
        let (sender, _) = channel();
        let _ = replace(&mut self.0, sender).send(());
    }
}
