use crate::effect::effect;
use crate::scope::untrack;
use crate::signal::{Signal, signal};
use crate::stored::{Stored, stored};
use crate::task::spawn;
use std::future::Future;

pub struct Resource<T> {
    loading: Signal<bool>,
    latest: Signal<Option<T>>,
    generation: Signal<u64>,
    queued: Stored<bool>,
}

impl<T> Clone for Resource<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Resource<T> {}

#[track_caller]
pub fn resource<T, U: 'static, V: Future<Output = U> + 'static>(
    source: impl Fn() -> T + 'static,
    fetch: impl Fn(T) -> V + 'static,
) -> Resource<U> {
    let loading = signal(true);
    let latest = signal(None);
    let generation = signal(0);
    let queued = stored(false);
    effect(move || {
        generation.get();
        let input = source();
        queued.set(false);
        loading.set_if_changed(true);
        let future = untrack(|| fetch(input));
        spawn(async move {
            latest.set(Some(future.await));
            if queued.update(std::mem::take) {
                generation.update(|g| *g += 1);
            } else {
                loading.set(false);
            }
        });
    });
    Resource {
        loading,
        latest,
        generation,
        queued,
    }
}

impl<T: Clone + 'static> Resource<T> {
    #[track_caller]
    pub fn get(&self) -> Option<T> {
        self.latest.get()
    }

    #[track_caller]
    pub fn loading(&self) -> bool {
        self.loading.get()
    }

    #[track_caller]
    pub fn with<U>(&self, f: impl FnOnce(Option<&T>) -> U) -> U {
        self.latest.with(|latest| f(latest.as_ref()))
    }

    #[track_caller]
    pub fn refetch(&self) {
        if self.loading.get_untracked() {
            self.queued.set(true);
        } else {
            self.generation.update(|g| *g += 1);
        }
    }
}
