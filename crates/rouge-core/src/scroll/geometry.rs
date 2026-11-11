use super::heights::{Heights, VariableHeights};
use super::{RowHeight, RowKey};
use crate::prop::Prop;
use rouge_reactive::{Memo, Signal, batch, effect, memo, signal};
use rouge_render::{ScrollState, Size};
use std::cell::RefCell;
use std::collections::HashMap;
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
            heights: Rc::new(RefCell::new(Heights::Uniform {
                height: 0.0,
                count: 0,
            })),
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
                state.update(|state| state.offset.y = offset);
            }
            self.version.update(|version| *version += 1);
        });
    }
}

pub(super) struct Keys<T> {
    pub(super) rows: Vec<RowKey<T>>,
    indices: HashMap<T, usize>,
}

impl<T: PartialEq> PartialEq for Keys<T> {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
    }
}

impl<T: Clone + Eq + Hash> Keys<T> {
    pub(super) fn new(rows: Vec<RowKey<T>>) -> Self {
        let indices: HashMap<_, _> = rows
            .iter()
            .enumerate()
            .map(|(i, row)| (row.id.clone(), i))
            .collect();
        assert_eq!(
            rows.len(),
            indices.len(),
            "virtual_list row IDs must be unique"
        );
        Self { rows, indices }
    }

    fn locate(&self, old: &Self, index: usize) -> Option<usize> {
        let find = |row: &RowKey<T>| self.indices.get(&row.id).copied();
        old.rows
            .get(index..)?
            .iter()
            .find_map(find)
            .or_else(|| old.rows[..index].iter().rev().find_map(find))
    }
}

pub(super) type Measurements<T> = Signal<HashMap<usize, (Option<RowKey<T>>, Signal<Size>)>>;

pub(super) fn configure_rows<T: Clone + Eq + Hash + 'static>(
    rows: &Rows,
    state: Signal<ScrollState>,
    current_count: Memo<usize>,
    current_keys: Option<Memo<Rc<Keys<T>>>>,
    row_height: Prop<RowHeight>,
    measured: bool,
    configured: Signal<u64>,
) {
    let rows = rows.clone();
    let mut previous: Option<Rc<Keys<T>>> = None;
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
            (0..count)
                .map(|i| {
                    let indexed = indexed.clone();
                    let keys = keys.clone();
                    memo(move || {
                        let same_shape =
                            if let Some((current, keys)) = current_keys.zip(keys.as_ref()) {
                                current.with_untracked(|current| Rc::ptr_eq(current, keys))
                            } else {
                                count == current_count.get_untracked()
                            };
                        same_shape.then(|| height(indexed(i)))
                    })
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let next = match input {
            RowHeight::Uniform(value) if !measured => Heights::Uniform {
                height: height(value),
                count,
            },
            _ => {
                let heights = match input {
                    RowHeight::Uniform(value) => vec![height(value); count],
                    RowHeight::Each(_) => estimates
                        .iter()
                        .map(|estimate| estimate.get_untracked().unwrap_or(minimum))
                        .collect(),
                };
                let cached = if let Heights::Variable(old) = &*rows.heights.borrow() {
                    old.measurements()
                        .iter()
                        .filter_map(|(&index, &height)| {
                            let target = match (&previous, &keys) {
                                (Some(previous), Some(keys)) => {
                                    let row = previous.rows.get(index)?;
                                    let &target = keys.indices.get(&row.id)?;
                                    (keys.rows[target] == *row).then_some(target)?
                                }
                                _ => (index < count).then_some(index)?,
                            };
                            Some((target, height))
                        })
                        .collect()
                } else {
                    HashMap::new()
                };
                Heights::Variable(VariableHeights::new(heights, cached))
            }
        };
        rows.update(state, |current, index| {
            let anchor = previous
                .as_ref()
                .zip(keys.as_ref())
                .and_then(|(old, new)| new.locate(old, index));
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
                    let Heights::Variable(heights) = heights else {
                        return (false, None);
                    };
                    (heights.set_estimate(index, value), None)
                });
            });
        }
    });
}

pub(super) type LayoutKey = (f32, f32, u64);

pub(super) fn observe_measurements<T: Clone + Eq + Hash + 'static>(
    rows: &Rows,
    state: Signal<ScrollState>,
    keys: Option<Memo<Rc<Keys<T>>>>,
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
                let Heights::Variable(heights) = heights else {
                    return (false, None);
                };
                let mut changed = invalidate && heights.clear_measurements();
                for (&index, (key, size)) in mounted {
                    if index >= count
                        || key.as_ref() != keys.as_ref().and_then(|keys| keys.rows.get(index))
                    {
                        continue;
                    }
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
