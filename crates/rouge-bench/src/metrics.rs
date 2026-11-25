use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Duration;

pub const TIMED: &str = "ms.";
pub const SPREAD: &str = ".spread";
pub const CPU: &str = "ms.cpu";

const GAUGES: [&str; 2] = ["frame.elements", "frame.layers"];

pub type Metrics = BTreeMap<String, f64>;
pub type Report = BTreeMap<String, Metrics>;

pub fn value(metrics: &Metrics, key: &str) -> f64 {
    metrics.get(key).copied().unwrap_or_default()
}

pub fn spread(metrics: &Metrics, key: &str) -> f64 {
    value(metrics, &format!("{key}{SPREAD}"))
}

pub fn settle(counts: Metrics, passes: &[Metrics]) -> Metrics {
    let mut settled = counts;
    settled.retain(|key, _| !key.starts_with(TIMED));
    let mut samples = BTreeMap::<&String, Vec<f64>>::new();
    for (key, value) in passes
        .iter()
        .flatten()
        .filter(|(key, _)| key.starts_with(TIMED))
    {
        samples.entry(key).or_default().push(*value);
    }
    for (key, mut values) in samples {
        values.sort_by(f64::total_cmp);
        let fastest = values[0];
        let median = values[values.len() / 2];
        settled.insert(key.clone(), fastest);
        settled.insert(format!("{key}{SPREAD}"), median - fastest);
    }
    for (key, value) in &mut settled {
        if key.starts_with(TIMED) || key.starts_with("heap.") {
            *value = (*value * 1e4).round() / 1e4;
        }
    }
    settled
}

pub fn scrape(metrics: &mut Metrics, root: &str, stats: &impl Debug) {
    let text = format!("{stats:?}");
    let (mut path, mut field) = (Vec::new(), root);
    for token in text.split_whitespace() {
        let token = token.trim_end_matches(',');
        if token == "{" {
            path.push(field);
        } else if token == "}" {
            path.pop();
        } else if let Some(name) = token.strip_suffix(':') {
            field = name;
        } else if let Ok(number) = token.parse::<u64>() {
            let key = format!("{}.{field}", path.join("."));
            let gauge = GAUGES.contains(&key.as_str());
            let total = metrics.entry(key).or_default();
            *total = if gauge { 0.0 } else { *total } + number as f64;
        }
    }
}

pub fn per_frame(metrics: &mut Metrics, frames: usize) {
    for (key, total) in metrics.iter_mut() {
        if !GAUGES.contains(&key.as_str()) {
            *total /= frames.max(1) as f64;
        }
    }
}

pub fn percentile(times: impl Iterator<Item = Duration>, p: f64) -> f64 {
    let mut sorted: Vec<Duration> = times.collect();
    sorted.sort();
    let index = ((sorted.len() as f64 * p) as usize).min(sorted.len().saturating_sub(1));
    sorted.get(index).copied().map_or(0.0, millis)
}

pub fn millis(time: Duration) -> f64 {
    time.as_secs_f64() * 1e3
}

pub fn amount(value: f64) -> String {
    match value.abs() {
        size if size >= 1e9 => format!("{:.2}G", value / 1e9),
        size if size >= 1e6 => format!("{:.2}M", value / 1e6),
        size if size >= 1e4 => format!("{:.1}k", value / 1e3),
        size if size >= 100.0 || size.fract() == 0.0 => format!("{value:.0}"),
        size if size >= 1.0 => format!("{value:.2}"),
        _ => format!("{value:.3}"),
    }
}
