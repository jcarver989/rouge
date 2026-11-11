use std::collections::HashMap;
use std::mem::take;

pub(super) enum Heights {
    Uniform { height: f32, count: usize },
    Variable(VariableHeights),
}

impl Heights {
    pub(super) fn count(&self) -> usize {
        match self {
            Self::Uniform { count, .. } => *count,
            Self::Variable(heights) => heights.estimates.len(),
        }
    }

    pub(super) fn top(&self, index: usize) -> f32 {
        let index = index.min(self.count());
        match self {
            Self::Uniform { height, .. } => index as f32 * height,
            Self::Variable(heights) => heights.prefix(index) as f32,
        }
    }

    pub(super) fn height(&self, index: usize) -> f32 {
        match self {
            Self::Uniform { height, .. } => *height,
            Self::Variable(heights) => heights.height(index),
        }
    }

    pub(super) fn starting_before(&self, y: f32, inclusive: bool) -> usize {
        let count = self.count();
        match self {
            Self::Uniform { height, .. } if *height > 0.0 => {
                let at = y / height;
                let index = if inclusive {
                    (at.floor() + 1.0).max(0.0)
                } else {
                    at.ceil().max(0.0)
                };
                (index as usize).min(count)
            }
            Self::Uniform { .. } => {
                if y > 0.0 || (inclusive && y == 0.0) {
                    count
                } else {
                    0
                }
            }
            Self::Variable(heights) => heights.starting_before(y, inclusive).min(count),
        }
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
}

pub(super) struct VariableHeights {
    estimates: Vec<f32>,
    measured: HashMap<usize, f32>,
    sums: Vec<f64>,
}

impl VariableHeights {
    pub(super) fn new(estimates: Vec<f32>, measured: HashMap<usize, f32>) -> Self {
        let count = estimates.len();
        let mut heights = Self {
            estimates,
            measured,
            sums: vec![0.0; count + 1],
        };
        for index in 1..=count {
            heights.sums[index] += f64::from(heights.height(index - 1));
            let parent = index + index.isolate_lowest_one();
            if parent <= count {
                heights.sums[parent] += heights.sums[index];
            }
        }
        heights
    }

    pub(super) fn measurements(&self) -> &HashMap<usize, f32> {
        &self.measured
    }

    fn height(&self, index: usize) -> f32 {
        self.measured
            .get(&index)
            .copied()
            .unwrap_or(self.estimates[index])
    }

    pub(super) fn set_estimate(&mut self, index: usize, height: f32) -> bool {
        let old = self.height(index);
        self.estimates[index] = height;
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

    fn update(&mut self, index: usize, old: f32) -> bool {
        let delta = f64::from(self.height(index)) - f64::from(old);
        if delta == 0.0 {
            return false;
        }
        let mut node = index + 1;
        while node < self.sums.len() {
            self.sums[node] += delta;
            node += node.isolate_lowest_one();
        }
        true
    }

    fn prefix(&self, mut count: usize) -> f64 {
        let mut top = 0.0;
        while count > 0 {
            top += self.sums[count];
            count -= count.isolate_lowest_one();
        }
        top
    }

    fn starting_before(&self, y: f32, inclusive: bool) -> usize {
        if y < 0.0 || (!inclusive && y == 0.0) {
            return 0;
        }
        let count = self.estimates.len();
        let mut step = count.checked_next_power_of_two().unwrap_or(count);
        let (mut index, mut top) = (0, 0.0);
        while step > 0 {
            let next = index + step;
            if next <= count {
                let candidate = top + self.sums[next];
                if (candidate as f32) < y || (inclusive && candidate as f32 == y) {
                    index = next;
                    top = candidate;
                }
            }
            step /= 2;
        }
        index + 1
    }
}
