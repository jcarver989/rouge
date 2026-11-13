use crate::fuzzy::fuzzy_filter;
use std::borrow::Cow;
use std::collections::HashMap;

pub trait PickerEntry {
    fn text(&self) -> Cow<'_, str>;

    fn group(&self) -> Option<&str> {
        None
    }

    fn disabled(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerOption {
    pub text: String,
    pub group: Option<String>,
    pub disabled: bool,
}

impl PickerOption {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            group: None,
            disabled: false,
        }
    }

    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl PickerEntry for PickerOption {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.text)
    }

    fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }

    fn disabled(&self) -> bool {
        self.disabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PickerRow {
    Group(usize),
    Option(usize),
}

impl PickerRow {
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Group(index) | Self::Option(index) => index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerState<T = PickerOption> {
    options: Vec<T>,
    filtered: bool,
    limit: Option<usize>,
    query: String,
    matches: Vec<usize>,
    highlighted: usize,
}

impl<T: PickerEntry> PickerState<T> {
    pub fn new(options: Vec<T>) -> Self {
        let mut state = Self {
            options,
            filtered: true,
            limit: None,
            query: String::new(),
            matches: Vec::new(),
            highlighted: 0,
        };
        state.refilter();
        state
    }

    pub fn unfiltered(mut self) -> Self {
        self.filtered = false;
        self.refilter();
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self.refilter();
        self
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.refilter();
    }

    pub fn options(&self) -> &[T] {
        &self.options
    }

    pub fn set_options(&mut self, options: Vec<T>) {
        self.options = options;
        self.refilter();
    }

    pub fn matches(&self) -> &[usize] {
        &self.matches
    }

    pub fn highlighted(&self) -> Option<usize> {
        self.matches
            .get(self.highlighted)
            .copied()
            .filter(|&index| !self.options[index].disabled())
    }

    pub fn highlight(&mut self, index: usize) -> bool {
        let Some(position) = self.matches.iter().position(|&matched| matched == index) else {
            return false;
        };
        let enabled = !self.options[index].disabled();
        if enabled {
            self.highlighted = position;
        }
        enabled
    }

    pub fn move_by(&mut self, delta: isize) -> bool {
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

    pub fn rows(&self) -> Vec<PickerRow> {
        let mut rows = Vec::with_capacity(self.matches.len());
        let mut group = None;
        for &index in &self.matches {
            let option_group = self.options[index].group();
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
        self.matches = if self.filtered {
            fuzzy_filter(&self.query, self.options.iter().map(PickerEntry::text))
        } else {
            (0..self.options.len()).collect()
        };
        let mut order = HashMap::new();
        for option in &self.options {
            if let Some(group) = option.group() {
                let next = order.len();
                order.entry(group).or_insert(next);
            }
        }
        let options = &self.options;
        self.matches
            .sort_by_key(|&index| options[index].group().map(|group| order[group]));
        if let Some(limit) = self.limit {
            self.matches.truncate(limit);
        }
        self.highlighted = self.enabled_from(0, 1).unwrap_or(0);
    }

    fn enabled_from(&self, start: usize, step: isize) -> Option<usize> {
        let mut position = start;
        loop {
            if !self.options[*self.matches.get(position)?].disabled() {
                return Some(position);
            }
            position = position.checked_add_signed(step)?;
        }
    }
}
