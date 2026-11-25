use crate::error::BenchError;
use crate::metrics::{CPU, Metrics, SPREAD, TIMED, value};

const SUPERLINEAR_TIME: f64 = 1.2;
const SUPERLINEAR_COUNT: f64 = 1.05;
const CPU_SHARE: f64 = 0.05;

pub struct Growth {
    pub key: String,
    pub values: Vec<f64>,
    pub exponent: f64,
}

impl Growth {
    pub fn is_superlinear(&self) -> bool {
        let limit = if self.key.starts_with(TIMED) {
            SUPERLINEAR_TIME
        } else {
            SUPERLINEAR_COUNT
        };
        self.exponent > limit
    }
}

pub fn sizes(requested: &[usize]) -> Result<Vec<usize>, BenchError> {
    let mut sizes = requested.to_vec();
    sizes.sort_unstable();
    sizes.dedup();
    if sizes.len() < 2 || sizes.contains(&0) {
        return Err(BenchError::TooFewSizes);
    }
    Ok(sizes)
}

pub fn growth(points: &[(usize, Metrics)]) -> Vec<Growth> {
    let Some((_, largest)) = points.last() else {
        return Vec::new();
    };
    let cpu = value(largest, CPU);
    let kept = largest.iter().filter(|(key, value)| {
        let noise = key.ends_with(SPREAD) || *key == "ms.cpu.p99" || *key == "frames";
        let trivial = key.starts_with(TIMED) && **value < cpu * CPU_SHARE;
        !noise && !trivial
    });
    let mut growth: Vec<Growth> = kept
        .filter_map(|(key, _)| {
            let values: Vec<f64> = points.iter().map(|(_, m)| value(m, key)).collect();
            let exponent = exponent(points.iter().map(|(size, _)| *size).zip(&values))?;
            Some(Growth {
                key: key.clone(),
                values,
                exponent,
            })
        })
        .collect();
    growth.sort_by(|a, b| {
        (a.key != CPU)
            .cmp(&(b.key != CPU))
            .then_with(|| b.exponent.total_cmp(&a.exponent))
    });
    growth
}

fn exponent<'a>(points: impl Iterator<Item = (usize, &'a f64)>) -> Option<f64> {
    let mut logs = Vec::new();
    for (size, value) in points {
        if *value <= 0.0 {
            return None;
        }
        logs.push(((size as f64).ln(), value.ln()));
    }
    let count = logs.len() as f64;
    let mean_x = logs.iter().map(|log| log.0).sum::<f64>() / count;
    let mean_y = logs.iter().map(|log| log.1).sum::<f64>() / count;
    let across: f64 = logs.iter().map(|l| (l.0 - mean_x) * (l.1 - mean_y)).sum();
    let along: f64 = logs.iter().map(|l| (l.0 - mean_x).powi(2)).sum();
    (along > 0.0).then(|| across / along)
}
