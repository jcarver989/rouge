use super::PickerItem;
use super::fuzzy::fuzzy_filter;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PickerRow {
    Group(usize),
    Option(usize),
}

impl PickerRow {
    pub(super) const fn index(self) -> usize {
        match self {
            Self::Group(index) | Self::Option(index) => index,
        }
    }
}

pub(super) struct PickerState {
    options: Vec<PickerItem>,
    groups: Vec<Option<usize>>,
    filtered: bool,
    limit: Option<usize>,
    query: String,
    matches: Vec<usize>,
    positions: Vec<Option<usize>>,
    highlighted: usize,
}

impl PickerState {
    pub(super) fn new(options: Vec<PickerItem>) -> Self {
        let mut state = Self {
            options: Vec::new(),
            groups: Vec::new(),
            filtered: true,
            limit: None,
            query: String::new(),
            matches: Vec::new(),
            positions: Vec::new(),
            highlighted: 0,
        };
        state.set_options(options);
        state
    }

    pub(super) fn unfiltered(mut self) -> Self {
        self.filtered = false;
        self.refilter();
        self
    }

    pub(super) fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self.refilter();
        self
    }

    pub(super) fn query(&self) -> &str {
        &self.query
    }

    pub(super) fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.refilter();
    }

    pub(super) fn options(&self) -> &[PickerItem] {
        &self.options
    }

    pub(super) fn set_options(&mut self, options: Vec<PickerItem>) {
        self.options = options;
        let mut order = HashMap::new();
        self.groups = self
            .options
            .iter()
            .map(|option| {
                option.group.as_deref().map(|group| {
                    let next = order.len();
                    *order.entry(group).or_insert(next)
                })
            })
            .collect();
        self.refilter();
    }

    pub(super) fn highlighted(&self) -> Option<usize> {
        self.matches
            .get(self.highlighted)
            .copied()
            .filter(|&index| !self.options[index].disabled)
    }

    pub(super) fn highlight(&mut self, index: usize) -> bool {
        let Some(position) = self.positions.get(index).copied().flatten() else {
            return false;
        };
        let enabled = !self.options[index].disabled;
        if enabled {
            self.highlighted = position;
        }
        enabled
    }

    pub(super) fn move_by(&mut self, delta: isize) -> bool {
        let Some(last) = self.matches.len().checked_sub(1) else {
            return false;
        };
        let target = self.highlighted.saturating_add_signed(delta).min(last);
        let step = if delta < 0 { -1 } else { 1 };
        let Some(next) = self
            .enabled_from(target, step)
            .or_else(|| self.enabled_from(target, -step))
        else {
            return false;
        };
        let moved = next != self.highlighted;
        self.highlighted = next;
        moved
    }

    pub(super) fn rows(&self) -> Vec<PickerRow> {
        let mut rows = Vec::with_capacity(self.matches.len());
        let mut group = None;
        for &index in &self.matches {
            let option_group = self.options[index].group.as_deref();
            if option_group != group {
                if option_group.is_some() {
                    rows.push(PickerRow::Group(index));
                }
                group = option_group;
            }
            rows.push(PickerRow::Option(index));
        }
        rows
    }

    fn refilter(&mut self) {
        if self.filtered {
            let keys = self.options.iter().map(|option| option.search.as_str());
            fuzzy_filter(&self.query, keys, &mut self.matches);
        } else {
            self.matches.clear();
            self.matches.extend(0..self.options.len());
        }
        let groups = &self.groups;
        self.matches.sort_by_key(|&index| groups[index]);
        if let Some(limit) = self.limit {
            self.matches.truncate(limit);
        }
        self.positions.clear();
        self.positions.resize(self.options.len(), None);
        for (position, &index) in self.matches.iter().enumerate() {
            self.positions[index] = Some(position);
        }
        self.highlighted = self.enabled_from(0, 1).unwrap_or(0);
    }

    fn enabled_from(&self, start: usize, step: isize) -> Option<usize> {
        let mut position = start;
        loop {
            if !self.options[*self.matches.get(position)?].disabled {
                return Some(position);
            }
            position = position.checked_add_signed(step)?;
        }
    }
}
