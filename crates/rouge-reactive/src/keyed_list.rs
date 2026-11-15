use crate::memo::{Memo, memo};
use crate::scope::{Scope, batch};
use crate::signal::{ReadSignal, Signal, signal};
use crate::stored::{Stored, stored};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::hash::Hash;

struct Entry<V> {
    value: V,
    scope: Scope,
    changed: Signal<()>,
}

struct State<K, V> {
    key: Box<dyn Fn(&V) -> K>,
    entries: FxHashMap<K, Entry<V>>,
    owner: Scope,
}

pub struct KeyedList<K: 'static, V: 'static> {
    state: Stored<State<K, V>>,
    order: Signal<Vec<K>>,
    revision: Signal<u64>,
}

impl<K, V> Clone for KeyedList<K, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K, V> Copy for KeyedList<K, V> {}

impl<K, V> Debug for KeyedList<K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_tuple("KeyedList").field(&self.order).finish()
    }
}

#[track_caller]
pub fn keyed_list<K: Eq + Hash + Clone + 'static, V: 'static>(
    key: impl Fn(&V) -> K + 'static,
) -> KeyedList<K, V> {
    KeyedList {
        state: stored(State {
            key: Box::new(key),
            entries: FxHashMap::default(),
            owner: Scope::new(),
        }),
        order: signal(Vec::new()),
        revision: signal(0),
    }
}

impl<K: Eq + Hash + Clone + 'static, V: 'static> KeyedList<K, V> {
    pub fn keys(&self) -> ReadSignal<Vec<K>> {
        self.order.read_only()
    }

    pub fn len(&self) -> usize {
        self.order.with(Vec::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn with<U>(&self, key: &K, f: impl FnOnce(&V) -> U) -> Option<U> {
        self.state.with(|state| {
            let entry = state.entries.get(key)?;
            entry.changed.with(|()| ());
            Some(f(&entry.value))
        })
    }

    pub fn with_values<U>(&self, f: impl FnOnce(&[&V]) -> U) -> U {
        self.revision.with(|_| ());
        self.order.with_untracked(|order| {
            self.state.with(|state| {
                let values: Vec<&V> = order.iter().map(|key| &state.entries[key].value).collect();
                f(&values)
            })
        })
    }

    #[track_caller]
    pub fn select<U: Clone + PartialEq + 'static>(
        self,
        key: K,
        get: impl Fn(&V) -> U + 'static,
    ) -> Memo<U> {
        let last = RefCell::new(None);
        memo(move || {
            if let Some(value) = self.with(&key, &get) {
                last.replace(Some(value));
            }
            last.borrow()
                .clone()
                .expect("rouge-reactive: a keyed list was read for a key it does not hold")
        })
    }

    pub fn push(&self, value: V) {
        self.extend([value]);
    }

    pub fn extend(&self, values: impl IntoIterator<Item = V>) {
        self.edit(|state| {
            let mut added = Vec::new();
            for value in values {
                let key = (state.key)(&value);
                if let Some(entry) = state.entries.get_mut(&key) {
                    entry.value = value;
                    entry.changed.set(());
                    continue;
                }
                let scope = state.owner.child();
                let changed = scope.run(|| signal(()));
                added.push(key.clone());
                let entry = Entry {
                    value,
                    scope,
                    changed,
                };
                state.entries.insert(key, entry);
            }
            if !added.is_empty() {
                self.order.update(|keys| keys.extend(added));
            }
        });
    }

    pub fn update(&self, key: &K, f: impl FnOnce(&mut V)) -> bool {
        self.edit(|state| {
            let entry = state.entries.get_mut(key);
            entry.map(|entry| entry.change(f)).is_some()
        })
    }

    pub fn update_all(&self, mut f: impl FnMut(&mut V)) {
        self.edit(|state| {
            for entry in state.entries.values_mut() {
                entry.change(&mut f);
            }
        });
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        self.edit(|state| {
            let entry = state.entries.remove(key)?;
            self.order.update(|keys| keys.retain(|other| other != key));
            Some(entry.release())
        })
    }

    pub fn retain(&self, mut keep: impl FnMut(&V) -> bool) {
        self.edit(|state| {
            let before = state.entries.len();
            for (_, entry) in state.entries.extract_if(|_, entry| !keep(&entry.value)) {
                entry.release();
            }
            if state.entries.len() != before {
                self.order
                    .update(|keys| keys.retain(|key| state.entries.contains_key(key)));
            }
        });
    }

    fn edit<U>(&self, f: impl FnOnce(&mut State<K, V>) -> U) -> U {
        batch(|| {
            let result = self.state.update(f);
            self.revision.update(|revision| *revision += 1);
            result
        })
    }
}

impl<V> Entry<V> {
    fn change(&mut self, f: impl FnOnce(&mut V)) {
        f(&mut self.value);
        self.changed.set(());
    }

    fn release(self) -> V {
        self.changed.set(());
        self.scope.dispose();
        self.value
    }
}
