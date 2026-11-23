use rustc_hash::FxHashMap;
use std::mem::take;

pub(super) struct Heights {
    base: Base,
    measured: FxHashMap<usize, f32>,
}

enum Base {
    Uniform {
        height: f32,
        count: usize,
        deltas: FxHashMap<usize, f64>,
    },
    Each {
        estimates: Vec<f32>,
        sums: Vec<f64>,
    },
}

impl Heights {
    pub(super) fn uniform(height: f32, count: usize, measured: FxHashMap<usize, f32>) -> Self {
        let mut heights = Self {
            base: Base::Uniform {
                height,
                count,
                deltas: FxHashMap::default(),
            },
            measured: FxHashMap::default(),
        };
        for (index, value) in measured {
            heights.measure(index, value);
        }
        heights
    }

    pub(super) fn each(estimates: Vec<f32>, measured: FxHashMap<usize, f32>) -> Self {
        let count = estimates.len();
        let mut sums = vec![0.0; count + 1];
        for index in 1..=count {
            sums[index] += f64::from(*measured.get(&(index - 1)).unwrap_or(&estimates[index - 1]));
            let parent = index + index.isolate_lowest_one();
            if parent <= count {
                sums[parent] += sums[index];
            }
        }
        Self {
            base: Base::Each { estimates, sums },
            measured,
        }
    }

    pub(super) fn count(&self) -> usize {
        match &self.base {
            Base::Uniform { count, .. } => *count,
            Base::Each { estimates, .. } => estimates.len(),
        }
    }

    pub(super) fn top(&self, index: usize) -> f32 {
        let mut count = index.min(self.count());
        if let Base::Uniform { height, deltas, .. } = &self.base
            && deltas.is_empty()
        {
            return (f64::from(*height) * count as f64) as f32;
        }
        let mut top = 0.0;
        while count > 0 {
            top += self.node(count);
            count -= count.isolate_lowest_one();
        }
        top as f32
    }

    pub(super) fn height(&self, index: usize) -> f32 {
        self.measured
            .get(&index)
            .copied()
            .unwrap_or_else(|| match &self.base {
                Base::Uniform { height, .. } => *height,
                Base::Each { estimates, .. } => estimates[index],
            })
    }

    pub(super) fn measurements(&self) -> &FxHashMap<usize, f32> {
        &self.measured
    }

    pub(super) fn set_estimate(&mut self, index: usize, height: f32) -> bool {
        let old = self.height(index);
        let Base::Each { estimates, .. } = &mut self.base else {
            return false;
        };
        estimates[index] = height;
        self.update(index, old)
    }

    pub(super) fn measure(&mut self, index: usize, height: f32) -> bool {
        let old = self.height(index);
        let added = self.measured.insert(index, height).is_none();
        self.update(index, old) || added
    }

    pub(super) fn clear_measurements(&mut self) -> bool {
        let measured = take(&mut self.measured);
        let changed = !measured.is_empty();
        for (index, old) in measured {
            self.update(index, old);
        }
        changed
    }

    pub(super) fn starting_before(&self, y: f32, inclusive: bool) -> usize {
        if y < 0.0 || (!inclusive && y == 0.0) {
            return 0;
        }
        let count = self.count();
        let mut step = count.checked_next_power_of_two().unwrap_or(count);
        let (mut index, mut top) = (0, 0.0);
        while step > 0 {
            let next = index + step;
            if next <= count {
                let candidate = top + self.node(next);
                if (candidate as f32) < y || (inclusive && candidate as f32 == y) {
                    index = next;
                    top = candidate;
                }
            }
            step /= 2;
        }
        (index + 1).min(count)
    }

    pub(super) fn next_visible(&self, index: usize) -> usize {
        if index >= self.count() || self.height(index) > 0.0 {
            return index.min(self.count());
        }
        let next = self.item_at(self.top(index));
        if self.height(next) > 0.0 {
            next
        } else {
            self.count()
        }
    }

    pub(super) fn item_at(&self, y: f32) -> usize {
        self.starting_before(y, true).saturating_sub(1)
    }

    fn node(&self, node: usize) -> f64 {
        match &self.base {
            Base::Each { sums, .. } => sums[node],
            Base::Uniform { height, deltas, .. } => {
                f64::from(*height) * node.isolate_lowest_one() as f64
                    + deltas.get(&node).copied().unwrap_or_default()
            }
        }
    }

    fn update(&mut self, index: usize, old: f32) -> bool {
        let delta = f64::from(self.height(index)) - f64::from(old);
        if delta == 0.0 {
            return false;
        }
        let mut node = index + 1;
        while node <= self.count() {
            match &mut self.base {
                Base::Each { sums, .. } => sums[node] += delta,
                Base::Uniform { deltas, .. } => {
                    let value = deltas.entry(node).or_default();
                    *value += delta;
                    if *value == 0.0 {
                        deltas.remove(&node);
                    }
                }
            }
            node += node.isolate_lowest_one();
        }
        true
    }
}
