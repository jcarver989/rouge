#![cfg(not(target_arch = "wasm32"))]

mod common;

use common::MetricsBuilder;
use rouge_bench::compare::{Change, Verdict, changes};
use rouge_bench::metrics::Metrics;

#[test]
fn a_build_compared_with_itself_has_no_changes() {
    let metrics = MetricsBuilder::default()
        .time("ms.cpu", 2.0, 0.1)
        .set("heap.allocations", 185.0)
        .set("frame.layouts", 1.0)
        .build();
    assert_eq!(changes(&metrics, &metrics), []);
}

#[test]
fn a_time_must_move_past_the_noise_and_the_spread() {
    assert_eq!(verdicts(&cpu(10.0, 0.0), &cpu(10.7, 0.0)), []);
    assert_eq!(
        verdicts(&cpu(10.0, 0.0), &cpu(11.0, 0.0)),
        [("ms.cpu".to_owned(), Verdict::Worse)]
    );
    assert_eq!(verdicts(&cpu(10.0, 1.0), &cpu(11.0, 0.0)), []);
    assert_eq!(
        verdicts(&cpu(10.0, 0.0), &cpu(5.0, 0.0)),
        [("ms.cpu".to_owned(), Verdict::Better)]
    );
}

#[test]
fn a_counter_changes_by_any_amount() {
    let layouts = |count| MetricsBuilder::default().set("frame.layouts", count);
    assert_eq!(
        verdicts(&layouts(1.0).build(), &layouts(1.0 + 1.0 / 1260.0).build()),
        [("frame.layouts".to_owned(), Verdict::Worse)]
    );
}

#[test]
fn allocations_may_drift_slightly() {
    let heap = |count| {
        let metrics = MetricsBuilder::default().set("heap.allocations", count);
        metrics.build()
    };
    assert_eq!(verdicts(&heap(10_000.0), &heap(10_040.0)), []);
    assert_eq!(
        verdicts(&heap(10_000.0), &heap(10_060.0)),
        [("heap.allocations".to_owned(), Verdict::Worse)]
    );
}

#[test]
fn more_cache_hits_are_neither_better_nor_worse() {
    let hits = |count| MetricsBuilder::default().set("frame.text.cache_hits", count);
    assert_eq!(
        verdicts(&hits(10.0).build(), &hits(20.0).build()),
        [("frame.text.cache_hits".to_owned(), Verdict::Changed)]
    );
}

#[test]
fn cpu_comes_first_and_then_the_largest_changes() {
    let of = |cpu, layouts, shapes| {
        MetricsBuilder::default()
            .time("ms.cpu", cpu, 0.0)
            .set("frame.layouts", layouts)
            .set("frame.text.shapes", shapes)
            .build()
    };
    let changes = changes(&of(10.0, 2.0, 1.0), &of(8.0, 1.0, 9.0));
    let keys: Vec<&str> = changes.iter().map(|change| change.key.as_str()).collect();
    assert_eq!(keys, ["ms.cpu", "frame.text.shapes", "frame.layouts"]);
}

#[test]
fn a_metric_only_one_build_has_is_left_out() {
    let after = MetricsBuilder::default().set("frame.new", 1.0).build();
    assert_eq!(changes(&Metrics::new(), &after), []);
}

fn cpu(time: f64, spread: f64) -> Metrics {
    MetricsBuilder::default()
        .time("ms.cpu", time, spread)
        .build()
}

fn verdicts(before: &Metrics, after: &Metrics) -> Vec<(String, Verdict)> {
    let pair = |change: Change| (change.key, change.verdict);
    changes(before, after).into_iter().map(pair).collect()
}
