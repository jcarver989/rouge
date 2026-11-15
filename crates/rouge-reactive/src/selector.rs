use crate::effect::effect;
use crate::scope::{Scope, batch, on_cleanup, untrack};
use crate::signal::{ReadSignal, Signal, signal};
use crate::stored::{Stored, stored};
use rustc_hash::FxHashMap;
use std::hash::Hash;
use std::mem::replace;

struct Entry {
    scope: Scope,
    signal: Signal<bool>,
    users: usize,
}

struct State<K> {
    current: K,
    keys: FxHashMap<K, Entry>,
    owner: Scope,
}

pub struct Selector<K: 'static>(Stored<State<K>>);

impl<K> Clone for Selector<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Copy for Selector<K> {}

#[track_caller]
pub fn selector<K: Eq + Hash + Clone + 'static>(source: impl Fn() -> K + 'static) -> Selector<K> {
    let state = stored(State {
        current: untrack(&source),
        keys: FxHashMap::default(),
        owner: Scope::new(),
    });
    effect(move || {
        let next = source();
        let changed = state.update(|state| {
            if state.current == next {
                return None;
            }
            let old = replace(&mut state.current, next.clone());
            let signal = |key: &K| state.keys.get(key).map(|entry| entry.signal);
            Some((signal(&old), signal(&next)))
        });
        if let Some((old, new)) = changed {
            batch(|| {
                if let Some(old) = old {
                    old.set(false);
                }
                if let Some(new) = new {
                    new.set(true);
                }
            });
        }
    });
    Selector(state)
}

impl<K: Eq + Hash + Clone + 'static> Selector<K> {
    #[track_caller]
    pub fn is(&self, key: K) -> ReadSignal<bool> {
        let state = self.0;
        let existing = state.update(|state| {
            state.keys.get_mut(&key).map(|entry| {
                entry.users += 1;
                entry.signal
            })
        });
        let signal = existing.unwrap_or_else(|| {
            let (owner, on) = state.with(|state| (state.owner, state.current == key));
            let scope = owner.child();
            let signal = scope.run(|| signal(on));
            let entry = Entry {
                scope,
                signal,
                users: 1,
            };
            state.update(|state| state.keys.insert(key.clone(), entry));
            signal
        });
        on_cleanup(move || {
            let released = state.try_with(|_| ()).and_then(|()| {
                state.update(|state| {
                    let entry = state.keys.get_mut(&key)?;
                    entry.users -= 1;
                    (entry.users == 0)
                        .then(|| state.keys.remove(&key))
                        .flatten()
                })
            });
            if let Some(entry) = released {
                entry.scope.dispose();
            }
        });
        signal.read_only()
    }
}
