use crate::metrics::{CPU, Metrics, SPREAD, TIMED, amount, spread};
use std::fmt::{Display, Formatter, Result as FmtResult};

const TIME_NOISE: f64 = 0.08;
const TIME_FLOOR: f64 = 0.005;
const SPREADS: f64 = 2.0;
const HEAP: &str = "heap.";
const HEAP_NOISE: f64 = 0.005;
const HEAP_FLOOR: f64 = 1.0;
const NEUTRAL: [&str; 6] = [
    "frames",
    "frame.text.cache_hits",
    "frame.elements",
    "frame.layers",
    "render.presented",
    "render.backdrops_reused",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Better,
    Worse,
    Changed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub key: String,
    pub before: f64,
    pub after: f64,
    pub verdict: Verdict,
}

impl Change {
    fn weight(&self) -> f64 {
        let (low, high) = (self.before.min(self.after), self.before.max(self.after));
        if low <= 0.0 {
            f64::INFINITY
        } else {
            high / low
        }
    }
}

impl Display for Change {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let verdict = match self.verdict {
            Verdict::Better => "better",
            Verdict::Worse => "WORSE",
            Verdict::Changed => "changed",
        };
        write!(
            f,
            "{:<34} {:>10} -> {:>10} {:>9}  {verdict}",
            self.key,
            amount(self.before),
            amount(self.after),
            percent(self.before, self.after),
        )
    }
}

pub fn percent(before: f64, after: f64) -> String {
    if before == 0.0 {
        return if after == 0.0 { "+0.0%" } else { "new" }.to_owned();
    }
    format!("{:+.1}%", (after - before) / before * 100.0)
}

pub fn changes(before: &Metrics, after: &Metrics) -> Vec<Change> {
    let shared = before.iter().filter(|(key, _)| !key.ends_with(SPREAD));
    let mut changes: Vec<Change> = shared
        .filter_map(|(key, old)| {
            let new = *after.get(key)?;
            ((new - old).abs() > noise(key, before, after)).then(|| Change {
                key: key.clone(),
                before: *old,
                after: new,
                verdict: verdict(key, *old, new),
            })
        })
        .collect();
    changes.sort_by(|a, b| {
        let rank = |change: &Change| (change.key != CPU, change.verdict == Verdict::Changed);
        rank(a)
            .cmp(&rank(b))
            .then_with(|| b.weight().total_cmp(&a.weight()))
    });
    changes
}

fn verdict(key: &str, before: f64, after: f64) -> Verdict {
    if NEUTRAL.contains(&key) {
        Verdict::Changed
    } else if after < before {
        Verdict::Better
    } else {
        Verdict::Worse
    }
}

fn noise(key: &str, before: &Metrics, after: &Metrics) -> f64 {
    let old = before.get(key).copied().unwrap_or_default();
    if key.starts_with(TIMED) {
        let spread = spread(before, key).max(spread(after, key));
        (old * TIME_NOISE).max(TIME_FLOOR).max(spread * SPREADS)
    } else if key.starts_with(HEAP) {
        (old * HEAP_NOISE).max(HEAP_FLOOR)
    } else {
        0.0
    }
}
