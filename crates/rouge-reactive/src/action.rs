use crate::signal::{Signal, signal};
use crate::stored::{Stored, stored};
use crate::task::spawn;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

type Run<T, U> = Rc<dyn Fn(T) -> Pin<Box<dyn Future<Output = U>>>>;

pub struct Action<T: 'static, U: 'static> {
    run: Stored<Run<T, U>>,
    pending: Signal<bool>,
    value: Signal<Option<U>>,
    latest: Stored<u64>,
}

impl<T, U> Clone for Action<T, U> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, U> Copy for Action<T, U> {}

#[track_caller]
pub fn action<T: 'static, U: 'static, V: Future<Output = U> + 'static>(
    run: impl Fn(T) -> V + 'static,
) -> Action<T, U> {
    let run: Run<T, U> = Rc::new(move |input| Box::pin(run(input)));
    Action {
        run: stored(run),
        pending: signal(false),
        value: signal(None),
        latest: stored(0),
    }
}

impl<T: 'static, U: 'static> Action<T, U> {
    pub fn dispatch(&self, input: T) {
        let Action {
            pending,
            value,
            latest,
            ..
        } = *self;
        let future = self.run.with(|run| run(input));
        let generation = latest.update(|latest| {
            *latest += 1;
            *latest
        });
        pending.set_if_changed(true);
        spawn(async move {
            let output = future.await;
            if latest.try_with(|latest| *latest == generation) == Some(true) {
                value.set(Some(output));
                pending.set(false);
            }
        });
    }

    #[track_caller]
    pub fn pending(&self) -> bool {
        self.pending.get()
    }

    #[track_caller]
    pub fn value(&self) -> Option<U>
    where
        U: Clone,
    {
        self.value.get()
    }

    #[track_caller]
    pub fn with<V>(&self, f: impl FnOnce(Option<&U>) -> V) -> V {
        self.value.with(|value| f(value.as_ref()))
    }
}
