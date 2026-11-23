use crate::memo::{Memo, memo};
use crate::scope::{Scope, batch};
use crate::signal::{ReadSignal, Signal, signal};
use crate::stored::{Stored, stored};
use imbl::shared_ptr::RcK;
use imbl::{GenericHashMap, GenericVector};
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::cell::RefCell;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::hash::Hash;
use std::ops::Range;
use std::rc::Rc;

#[derive(Clone)]
pub struct KeyOrder<K> {
    keys: GenericVector<(K, u64), RcK>,
    ranks: Ranks<K>,
    origin: isize,
}

type SharedRanks<K> = GenericHashMap<K, isize, FxBuildHasher, RcK>;

#[derive(Clone)]
enum Ranks<K> {
    Flat(Rc<FxHashMap<K, isize>>),
    Shared(SharedRanks<K>),
}

impl<K: Clone + Eq + Hash> Ranks<K> {
    fn get(&self, key: &K) -> Option<isize> {
        match self {
            Self::Flat(ranks) => ranks.get(key).copied(),
            Self::Shared(ranks) => ranks.get(key).copied(),
        }
    }

    fn edit(&mut self) -> &mut SharedRanks<K> {
        if let Self::Flat(ranks) = self {
            *self = Self::Shared(
                ranks
                    .iter()
                    .map(|(key, rank)| (key.clone(), *rank))
                    .collect(),
            );
        }
        match self {
            Self::Shared(ranks) => ranks,
            Self::Flat(_) => unreachable!(),
        }
    }
}

impl<K: PartialEq> PartialEq for KeyOrder<K> {
    fn eq(&self, other: &Self) -> bool {
        self.keys.ptr_eq(&other.keys) || self.keys == other.keys
    }
}

impl<K: Clone + Eq + Hash> FromIterator<(K, u64)> for KeyOrder<K> {
    fn from_iter<T: IntoIterator<Item = (K, u64)>>(entries: T) -> Self {
        let keys: GenericVector<_, RcK> = entries.into_iter().collect();
        let ranks: FxHashMap<K, isize> = keys
            .iter()
            .zip(0..)
            .map(|((key, _), rank)| (key.clone(), rank))
            .collect();
        assert_eq!(ranks.len(), keys.len(), "keys must be unique");
        Self {
            keys,
            ranks: Ranks::Flat(Rc::new(ranks)),
            origin: 0,
        }
    }
}

impl<K: Clone + Eq + Hash> KeyOrder<K> {
    pub fn len(&self) -> usize {
        self.keys.len()
    }
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<&K> {
        self.entry(index).map(|(key, _)| key)
    }
    pub fn entry(&self, index: usize) -> Option<&(K, u64)> {
        self.keys.get(index)
    }
    pub fn index_of(&self, key: &K) -> Option<usize> {
        self.ranks
            .get(key)
            .map(|rank| (rank - self.origin) as usize)
    }
    pub fn position(&self, (key, generation): &(K, u64)) -> Option<usize> {
        self.index_of(key)
            .filter(|&index| self.keys[index].1 == *generation)
    }

    fn push(&mut self, key: K, generation: u64, front: bool) {
        if front {
            self.origin -= 1;
            self.ranks.edit().insert(key.clone(), self.origin);
            self.keys.push_front((key, generation));
        } else {
            let rank = self.origin + self.len().cast_signed();
            self.ranks.edit().insert(key.clone(), rank);
            self.keys.push_back((key, generation));
        }
    }

    fn reindex(&mut self, range: Range<usize>) {
        for index in range {
            let rank = self.origin + index.cast_signed();
            self.ranks.edit().insert(self.keys[index].0.clone(), rank);
        }
    }

    fn remove(&mut self, index: usize) {
        let (key, _) = self.keys.remove(index);
        self.ranks.edit().remove(&key);
        if index < self.len() - index {
            self.origin += 1;
            self.reindex(0..index);
        } else {
            self.reindex(index..self.len());
        }
    }

    fn move_to(&mut self, from: usize, to: usize) {
        let key = self.keys.remove(from);
        self.keys.insert(to, key);
        self.reindex(from.min(to)..from.max(to) + 1);
    }
}

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
    order: Signal<Rc<KeyOrder<K>>>,
    keys: Memo<Vec<K>>,
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
    let order = signal(Rc::new(KeyOrder::<K>::from_iter([])));
    let keys =
        memo(move || order.with(|order| order.keys.iter().map(|(key, _)| key.clone()).collect()));
    KeyedList {
        state: stored(State {
            key: Box::new(key),
            entries: FxHashMap::default(),
            owner: Scope::new(),
        }),
        order,
        keys,
        revision: signal(0),
    }
}

impl<K: Eq + Hash + Clone + 'static, V: 'static> KeyedList<K, V> {
    pub fn keys(&self) -> ReadSignal<Vec<K>> {
        self.keys
    }

    pub fn order(&self) -> ReadSignal<Rc<KeyOrder<K>>> {
        self.order.read_only()
    }

    pub fn len(&self) -> usize {
        self.order.with(|order| order.len())
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
                let values: Vec<&V> = order
                    .keys
                    .iter()
                    .map(|(key, _)| &state.entries[key].value)
                    .collect();
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

    pub fn prepend(&self, value: V) {
        self.add([value], true);
    }

    pub fn extend(&self, values: impl IntoIterator<Item = V>) {
        self.add(values, false);
    }

    fn add(&self, values: impl IntoIterator<Item = V>, front: bool) {
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
                added.push((key.clone(), changed.id()));
                let entry = Entry {
                    value,
                    scope,
                    changed,
                };
                state.entries.insert(key, entry);
            }
            if !added.is_empty() {
                self.order.update(|order| {
                    let order = Rc::make_mut(order);
                    for (key, generation) in added {
                        order.push(key, generation, front);
                    }
                });
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
            self.order.update(|order| {
                let index = order.index_of(key).expect("entry has an order rank");
                Rc::make_mut(order).remove(index);
            });
            Some(entry.release())
        })
    }

    pub fn move_to(&self, key: &K, index: usize) -> bool {
        let Some((from, to)) = self.order.with_untracked(|order| {
            Some((
                order.index_of(key)?,
                index.min(order.len().saturating_sub(1)),
            ))
        }) else {
            return false;
        };
        if from != to {
            self.edit(|_| {
                self.order
                    .update(|order| Rc::make_mut(order).move_to(from, to));
            });
        }
        true
    }

    pub fn retain(&self, mut keep: impl FnMut(&V) -> bool) {
        self.edit(|state| {
            let before = state.entries.len();
            for (_, entry) in state.entries.extract_if(|_, entry| !keep(&entry.value)) {
                entry.release();
            }
            if state.entries.len() != before {
                self.order.update(|order| {
                    let kept = order
                        .keys
                        .iter()
                        .filter(|(key, _)| state.entries.contains_key(key));
                    *order = Rc::new(kept.cloned().collect());
                });
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
