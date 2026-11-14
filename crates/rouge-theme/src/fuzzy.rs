use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use std::{cmp::Reverse, fmt};

#[derive(Default)]
pub(crate) struct FuzzyFilter {
    matcher: Option<Matcher>,
    pattern: Pattern,
    haystack: Vec<char>,
    scored: Vec<(u32, usize)>,
}

impl FuzzyFilter {
    pub(crate) fn filter<T: AsRef<str>>(
        &mut self,
        query: &str,
        keys: impl ExactSizeIterator<Item = T>,
        output: &mut Vec<usize>,
    ) {
        output.clear();
        if query.trim().is_empty() {
            output.extend(0..keys.len());
            return;
        }
        let matcher = self
            .matcher
            .get_or_insert_with(|| Matcher::new(Config::DEFAULT));
        self.pattern
            .reparse(query, CaseMatching::Smart, Normalization::Smart);
        self.scored.clear();
        for (index, key) in keys.enumerate() {
            let haystack = Utf32Str::new(key.as_ref(), &mut self.haystack);
            if let Some(score) = self.pattern.score(haystack, matcher) {
                self.scored.push((score, index));
            }
        }
        self.scored.sort_by_key(|&(score, _)| Reverse(score));
        output.extend(self.scored.iter().map(|&(_, index)| index));
    }
}

impl Clone for FuzzyFilter {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for FuzzyFilter {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for FuzzyFilter {}

impl fmt::Debug for FuzzyFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FuzzyFilter").finish_non_exhaustive()
    }
}
