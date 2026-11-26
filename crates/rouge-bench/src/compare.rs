use rouge_bench::bench::Metrics;
use rouge_bench::error::BenchError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) const SCHEMA: u32 = 1;
pub(crate) const CPU: &str = "ms.cpu";
const PRIMARY: [&str; 5] = [
    CPU,
    "ms.gpu_wait",
    "heap.allocations",
    "render.instance_bytes_uploaded",
    "memory.bytes",
];

const EXACT: [&str; 3] = ["frame.", "render.", "memory."];

#[derive(Serialize, Deserialize)]
pub(crate) struct Report {
    pub schema: u32,
    pub environment: String,
    pub passes: usize,
    pub cases: BTreeMap<String, Vec<Metrics>>,
}

impl Report {
    fn values(&self, case: &str, key: &str) -> Result<(f64, f64), BenchError> {
        let invalid =
            || BenchError::Report(format!("{case}: missing or invalid samples for {key}"));
        let samples = self
            .cases
            .get(case)
            .filter(|samples| !samples.is_empty() && samples.len() == self.passes)
            .ok_or_else(invalid)?;
        let mut values = samples
            .iter()
            .map(|s| {
                s.get(key)
                    .copied()
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .ok_or_else(invalid)
            })
            .collect::<Result<Vec<_>, _>>()?;
        values.sort_unstable_by(f64::total_cmp);
        Ok((
            values[values.len() / 2],
            values[values.len() - 1] - values[0],
        ))
    }

    pub(crate) fn summary(&self, case: &str) -> Result<(), BenchError> {
        print!("{case:<28}");
        for key in PRIMARY {
            print!(" {key}={:.3}", self.values(case, key)?.0);
        }
        println!();
        Ok(())
    }

    pub(crate) fn compare(&self, other: &Self, requested: &[String]) -> Result<bool, BenchError> {
        if self.schema != SCHEMA
            || other.schema != SCHEMA
            || self.environment != other.environment
            || self.passes != other.passes
        {
            return Err(BenchError::Report(
                "incompatible schema, environment or pass count".into(),
            ));
        }
        let all;
        let cases = if requested.is_empty() {
            if self.cases.is_empty() || self.cases.keys().ne(other.cases.keys()) {
                return Err(BenchError::Report(
                    "empty or different case sets; name exact cases to compare a subset".into(),
                ));
            }
            all = self.cases.keys().cloned().collect::<Vec<_>>();
            &all
        } else {
            requested
        };
        let mut worse = false;
        for case in cases {
            let (frames, spread) = self.values(case, "frames")?;
            if frames == 0.0
                || frames.fract() != 0.0
                || spread != 0.0
                || other.values(case, "frames")? != (frames, 0.0)
            {
                return Err(BenchError::Report(format!(
                    "{case}: measured frame counts must be equal positive integers"
                )));
            }
            println!("{case} ({frames} frames)");
            let sample = self.cases.get(case).and_then(|samples| samples.first());
            for key in sample.into_iter().flat_map(Metrics::keys) {
                let (old, a) = self.values(case, key)?;
                let (new, b) = other.values(case, key)?;
                let beyond = |tolerance: f64| (tolerance * old).max(a).max(b);
                let verdict = if key == CPU {
                    (new - old > beyond(0.08).max(0.005)).then_some("REGRESSION")
                } else {
                    let tolerance = if key.starts_with("heap.") {
                        Some(0.05)
                    } else {
                        EXACT
                            .iter()
                            .any(|prefix| key.starts_with(prefix))
                            .then_some(0.0)
                    };
                    tolerance
                        .filter(|tolerance| (new - old).abs() > beyond(*tolerance))
                        .map(|_| "CHANGED")
                };
                worse |= verdict.is_some();
                if verdict.is_some() || PRIMARY.contains(&key.as_str()) {
                    println!(
                        "  {key:<32} {old:>12.3} -> {new:>12.3} {}",
                        verdict.unwrap_or_default()
                    );
                }
            }
        }
        Ok(worse)
    }
}
