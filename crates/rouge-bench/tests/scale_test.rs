#![cfg(not(target_arch = "wasm32"))]

mod common;

use common::{MetricsBuilder, TestResult};
use rouge_bench::metrics::Metrics;
use rouge_bench::scale::{Growth, growth, sizes};

#[test]
fn growth_tells_linear_from_quadratic() -> TestResult {
    let growth = growth(&points(|size| {
        MetricsBuilder::default()
            .set("ms.cpu", size * size)
            .set("frame.text.shapes", size * 2.0)
            .build()
    }));
    let cpu = named(&growth, "ms.cpu")?;
    let shapes = named(&growth, "frame.text.shapes")?;
    assert!((cpu.exponent - 2.0).abs() < 1e-9 && cpu.is_superlinear());
    assert!((shapes.exponent - 1.0).abs() < 1e-9 && !shapes.is_superlinear());
    Ok(())
}

#[test]
fn times_get_more_slack_than_counters() -> TestResult {
    let growth = growth(&points(|size| {
        MetricsBuilder::default()
            .set("ms.cpu", size.powf(1.15))
            .set("frame.patches", size.powf(1.15))
            .build()
    }));
    assert!(!named(&growth, "ms.cpu")?.is_superlinear());
    assert!(named(&growth, "frame.patches")?.is_superlinear());
    Ok(())
}

#[test]
fn spans_too_small_to_matter_are_left_out() {
    let growth = growth(&points(|size| {
        MetricsBuilder::default()
            .set("ms.cpu", size)
            .set("ms.span.scene", size * 0.01)
            .build()
    }));
    let keys: Vec<&str> = growth.iter().map(|growth| growth.key.as_str()).collect();
    assert_eq!(keys, ["ms.cpu"]);
}

#[test]
fn a_scale_needs_two_sizes_above_zero() -> TestResult {
    assert_eq!(sizes(&[3000, 1000, 3000])?, [1000, 3000]);
    assert!(sizes(&[1000, 1000]).is_err());
    assert!(sizes(&[0, 1000]).is_err());
    Ok(())
}

fn points(metrics: impl Fn(f64) -> Metrics) -> Vec<(usize, Metrics)> {
    let sizes = [1000, 3000, 10_000];
    sizes.map(|size| (size, metrics(size as f64))).to_vec()
}

fn named<'a>(growth: &'a [Growth], key: &str) -> Result<&'a Growth, String> {
    let found = growth.iter().find(|growth| growth.key == key);
    found.ok_or_else(|| format!("{key} has no growth"))
}
