#![cfg(feature = "tokio-rt")]

use rouge_reactive::testing::{Driver, TestResult};
use rouge_reactive::{Scope, TaskError, signal, spawn, spawn_on};
use std::future::pending;
use std::thread::{self, ThreadId};
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::oneshot::{Sender, channel};

#[test]
fn tokio_io_and_completion_cross_back_to_the_ui_executor() -> TestResult {
    let driver = Driver::new();
    let runtime = tokio_runtime()?;
    let owner = thread::current().id();
    let completed = signal(None::<(ThreadId, usize)>);
    let updates = signal(0);
    let sender = updates.sender();
    let task = spawn_on(runtime.handle().clone(), async move {
        tokio::time::sleep(Duration::from_millis(1)).await;
        sender.set(4);
        7usize
    });
    spawn(async move {
        let value = task.await.expect("Tokio task completed");
        completed.set(Some((thread::current().id(), value)));
    });
    driver.run_until(|| completed.get().is_some())?;
    assert_eq!(completed.get(), Some((owner, 7)));
    assert_eq!(updates.get(), 4);
    Ok(())
}

#[test]
fn dropping_bridge_or_its_scope_aborts_tokio_and_drops_the_future() -> TestResult {
    let driver = Driver::new();
    let runtime = tokio_runtime()?;
    for dispose_scope in [false, true] {
        let scope = Scope::root();
        let (started, ready) = channel();
        let (dropped, closed) = channel();
        let task = scope.run(|| {
            spawn_on(runtime.handle().clone(), async move {
                let _drop = DropNotice(dropped);
                let _ = started.send(());
                pending::<()>().await;
            })
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
    let (ran, ready) = channel();
    let task = spawn_on(runtime.handle().clone(), async move {
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
    let task = spawn_on(runtime.handle().clone(), async {
        panic!("intentional task failure")
    });
    spawn(async move {
        failed.set(matches!(task.await, Err(TaskError::Tokio(_))));
    });
    driver.run_until(|| failed.get())?;
    Ok(())
}

#[test]
fn local_tasks_share_the_typed_join_api_without_changing_scope_owned_lifetimes() -> TestResult {
    let driver = Driver::new();
    let value = signal(0);
    let task = spawn(async { 9 });
    spawn(async move {
        value.set(task.await.expect("local task result"));
    });
    driver.run_until(|| value.get() == 9)?;
    Ok(())
}

fn tokio_runtime() -> Result<Runtime, std::io::Error> {
    Builder::new_multi_thread()
        .worker_threads(1)
        .enable_time()
        .build()
}

struct DropNotice(Sender<()>);

impl Drop for DropNotice {
    fn drop(&mut self) {
        let (sender, _) = channel();
        let _ = std::mem::replace(&mut self.0, sender).send(());
    }
}
