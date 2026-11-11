use crate::runtime::{Graph, try_with_rt};
use std::ops::Sub;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReactiveStats {
    pub nodes_created: u64,
    pub nodes_disposed: u64,
    pub effects_run: u64,
    pub memos_run: u64,
}

impl Sub for ReactiveStats {
    type Output = ReactiveStats;

    fn sub(self, earlier: ReactiveStats) -> ReactiveStats {
        ReactiveStats {
            nodes_created: self.nodes_created - earlier.nodes_created,
            nodes_disposed: self.nodes_disposed - earlier.nodes_disposed,
            effects_run: self.effects_run - earlier.effects_run,
            memos_run: self.memos_run - earlier.memos_run,
        }
    }
}

pub fn stats() -> ReactiveStats {
    try_with_rt(Graph::stats).unwrap_or_default()
}
