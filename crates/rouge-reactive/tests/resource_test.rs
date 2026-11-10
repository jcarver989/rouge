use rouge_reactive::testing::{Driver, TestResult, counter, recorder};
use rouge_reactive::{Scope, effect, poll_tasks, resource, signal};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use tokio::sync::oneshot::{Receiver, Sender, channel};

#[test]
fn resource_goes_from_loading_to_ready_and_reads_are_tracked() {
    let _driver = Driver::new();
    let r = resource(|| 2, |n| async move { n * 10 });
    let (seen, record) = recorder();
    effect(move || record((r.get(), r.loading())));
    assert!(poll_tasks());
    assert_eq!(*seen.borrow(), [(None, true), (Some(20), false)]);
}

#[test]
fn resource_refetches_when_its_source_changes() {
    let _driver = Driver::new();
    let id = signal(1);
    let (fetches, bump) = counter();
    let r = resource(
        move || id.get(),
        move |id| {
            bump();
            async move { format!("user {id}") }
        },
    );
    poll_tasks();
    assert_eq!(r.get().as_deref(), Some("user 1"));
    id.set(2);
    assert_eq!(fetches.get(), 2);
    assert!(r.loading());
    assert_eq!(
        r.get().as_deref(),
        Some("user 1"),
        "the previous value is kept while re-fetching"
    );
    poll_tasks();
    assert_eq!((r.loading(), r.get().as_deref()), (false, Some("user 2")));
}

#[test]
fn refetch_runs_the_fetcher_again() {
    let _driver = Driver::new();
    let (fetches, bump) = counter();
    let fetches2 = fetches.clone();
    let r = resource(
        || (),
        move |()| {
            bump();
            let n = fetches2.get();
            async move { n }
        },
    );
    poll_tasks();
    assert_eq!(r.get(), Some(1));
    r.refetch();
    assert!(r.loading());
    poll_tasks();
    assert_eq!((fetches.get(), r.loading(), r.get()), (2, false, Some(2)));
}

#[test]
fn fetcher_reads_are_not_tracked() {
    let _driver = Driver::new();
    let untracked = signal(1);
    let (fetches, bump) = counter();
    let r = resource(
        || (),
        move |()| {
            bump();
            let n = untracked.get();
            async move { n }
        },
    );
    poll_tasks();
    untracked.set(2);
    poll_tasks();
    assert_eq!((fetches.get(), r.get()), (1, Some(1)));
}

#[test]
fn stale_fetch_never_overwrites_a_newer_result() -> TestResult {
    let _driver = Driver::new();
    let (gates, replies) = gates(2);
    let [old, new] = gates.try_into().map_err(|_| "expected two gates")?;
    let id = signal(1);
    let r = resource(move || id.get(), move |_| next_reply(&replies));
    poll_tasks();
    id.set(2);
    poll_tasks();
    new.send("new")?;
    poll_tasks();
    assert_eq!((r.loading(), r.get()), (false, Some(Some("new"))));
    assert!(old.send("old").is_err(), "the stale fetch was cancelled");
    poll_tasks();
    assert_eq!((r.loading(), r.get()), (false, Some(Some("new"))));
    Ok(())
}

#[test]
fn disposing_the_scope_cancels_the_fetch() -> TestResult {
    let _driver = Driver::new();
    let (mut gates, replies) = gates(1);
    let scope = Scope::root();
    scope.run(move || resource(|| (), move |()| next_reply(&replies)));
    poll_tasks();
    scope.dispose();
    let gate = gates.pop().ok_or("expected one gate")?;
    assert!(gate.send("late").is_err());
    Ok(())
}

type Replies = Rc<RefCell<VecDeque<Receiver<&'static str>>>>;

fn gates(count: usize) -> (Vec<Sender<&'static str>>, Replies) {
    let (gates, replies): (Vec<_>, VecDeque<_>) = (0..count).map(|_| channel()).unzip();
    (gates, Rc::new(RefCell::new(replies)))
}

fn next_reply(replies: &Replies) -> impl Future<Output = Option<&'static str>> + use<> {
    let reply = replies.borrow_mut().pop_front();
    async move { reply?.await.ok() }
}
