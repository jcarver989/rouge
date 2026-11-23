use super::RowHeight;
use super::heights::Heights;
use crate::compositor::ScrollState;
use crate::prop::Prop;
use rouge_reactive::{KeyOrder, Memo, ReadSignal, Signal, batch, effect, memo, signal};
use rouge_render::{Point, Size};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

#[derive(Clone)]
pub(super) struct Rows {
    heights: Rc<RefCell<Heights>>,
    pub(super) version: Signal<u64>,
}

impl PartialEq for Rows {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
    }
}

impl Rows {
    pub(super) fn new() -> Self {
        Self {
            heights: Rc::new(RefCell::new(Heights::uniform(0.0, 0, FxHashMap::default()))),
            version: signal(0),
        }
    }

    pub(super) fn with<T>(&self, read: impl FnOnce(&Heights) -> T) -> T {
        self.version.get();
        read(&self.heights.borrow())
    }

    fn update(
        &self,
        state: Signal<ScrollState>,
        change: impl FnOnce(&mut Heights, usize) -> (bool, Option<usize>),
    ) {
        let current = state.get_untracked();
        let y = current.offset.y;
        let mut heights = self.heights.borrow_mut();
        let was_empty = heights.count() == 0;
        let index = heights.item_at(y);
        let within = (y - heights.top(index)).max(0.0);
        let (changed, anchor) = change(&mut heights, index);
        if !changed {
            return;
        }
        let offset = if heights.count() == 0 {
            0.0
        } else if was_empty || y < 0.0 {
            y
        } else {
            let index = anchor.unwrap_or(index).min(heights.count() - 1);
            let within = within.min(heights.height(index));
            heights.top(index) + within
        };
        drop(heights);
        batch(|| {
            if y != offset {
                state.update(|state| state.anchor_to(Point::new(state.offset.x, offset)));
            }
            self.version.update(|version| *version += 1);
        });
    }
}

pub(super) type Keys<T> = ReadSignal<Rc<KeyOrder<T>>>;
pub(super) type MountKey<T> = Result<(T, u64), usize>;
pub(super) type Measurements<T> = Signal<FxHashMap<MountKey<T>, Signal<Size>>>;

pub(super) fn configure_rows<T: Clone + Eq + Hash + 'static>(
    rows: &Rows,
    state: Signal<ScrollState>,
    current_count: Memo<usize>,
    current_keys: Option<Keys<T>>,
    row_height: Prop<RowHeight>,
    measured: bool,
    configured: Signal<u64>,
) {
    let rows = rows.clone();
    let mut previous: Option<Rc<KeyOrder<T>>> = None;
    effect(move || {
        let count = current_count.get();
        let keys = current_keys.map(|keys| keys.get());
        let input = row_height.get();
        let minimum = if measured { 1.0 } else { 0.0 };
        let height = move |height: f32| {
            if height.is_finite() {
                height.max(minimum)
            } else {
                minimum
            }
        };
        let estimates = if let RowHeight::Each(indexed) = &input {
            Vec::from_fn(count, |i| {
                let indexed = indexed.clone();
                let keys = keys.clone();
                memo(move || {
                    let same_shape = if let Some((current, keys)) = current_keys.zip(keys.as_ref())
                    {
                        current.with_untracked(|current| Rc::ptr_eq(current, keys))
                    } else {
                        count == current_count.get_untracked()
                    };
                    same_shape.then(|| height(indexed(i)))
                })
            })
        } else {
            Vec::new()
        };
        let cached = rows
            .heights
            .borrow()
            .measurements()
            .iter()
            .filter_map(|(&index, &height)| {
                let target = match (&previous, &keys) {
                    (Some(previous), Some(keys)) => keys.position(previous.entry(index)?)?,
                    _ => (index < count).then_some(index)?,
                };
                Some((target, height))
            })
            .collect();
        let next = match input {
            RowHeight::Uniform(value) => Heights::uniform(height(value), count, cached),
            RowHeight::Each(_) => Heights::each(
                estimates
                    .iter()
                    .map(|estimate| estimate.get_untracked().unwrap_or(minimum))
                    .collect(),
                cached,
            ),
        };
        rows.update(state, |current, index| {
            let anchor = previous
                .as_ref()
                .zip(keys.as_ref())
                .and_then(|(old, new)| locate(old, new, index));
            *current = next;
            (true, anchor)
        });
        previous = keys;
        configured.update(|version| *version += 1);
        for (index, estimate) in estimates.into_iter().enumerate() {
            let rows = rows.clone();
            let mut last = estimate.get_untracked();
            effect(move || {
                let Some(value) = estimate.get() else { return };
                if last.replace(value) == Some(value) {
                    return;
                }
                rows.update(state, |heights, _| {
                    (heights.set_estimate(index, value), None)
                });
            });
        }
    });
}

pub(super) type LayoutKey = (f32, u64);

pub(super) fn observe_measurements<T: Clone + Eq + Hash + 'static>(
    rows: &Rows,
    state: Signal<ScrollState>,
    keys: Option<Keys<T>>,
    configured: Signal<u64>,
    mounted: Measurements<T>,
    layout: Memo<LayoutKey>,
) {
    let rows = rows.clone();
    let mut previous = None;
    effect(move || {
        configured.get();
        let keys = keys.map(|keys| keys.get());
        let current = layout.get();
        let invalidate = previous.replace(current) != Some(current);
        mounted.with(|mounted| {
            rows.update(state, |heights, _| {
                let count = heights.count();
                let mut changed = invalidate && heights.clear_measurements();
                for (key, size) in mounted {
                    let index = match (key, &keys) {
                        (Err(index), None) => Some(*index),
                        (Ok(key), Some(keys)) => keys.position(key),
                        _ => None,
                    };
                    let Some(index) = index.filter(|&index| index < count) else {
                        continue;
                    };
                    let size = size.get();
                    if size.width == current.0
                        && current.0 > 0.0
                        && size.height.is_finite()
                        && size.height >= 1.0
                    {
                        changed |= heights.measure(index, size.height);
                    }
                }
                (changed, None)
            });
        });
    });
}

fn locate<T: Clone + Eq + Hash>(
    old: &KeyOrder<T>,
    new: &KeyOrder<T>,
    index: usize,
) -> Option<usize> {
    let index = index.min(old.len());
    (index..old.len())
        .chain((0..index).rev())
        .find_map(|at| new.index_of(old.get(at)?))
}
