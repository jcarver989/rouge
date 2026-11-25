#![cfg(not(target_arch = "wasm32"))]

mod common;

use common::MetricsBuilder;
use rouge::core::{FrameStats, TextStats};
use rouge_bench::metrics::{Metrics, amount, per_frame, scrape, settle, spread, value};

#[test]
fn counters_add_up_per_frame_and_states_do_not() {
    let mut metrics = Metrics::new();
    for elements in [10, 30] {
        let stats = FrameStats {
            layouts: 3,
            elements,
            text: TextStats {
                shapes: 1,
                ..TextStats::default()
            },
            ..FrameStats::default()
        };
        scrape(&mut metrics, "frame", &stats);
    }
    per_frame(&mut metrics, 2);
    assert_eq!(value(&metrics, "frame.layouts"), 3.0);
    assert_eq!(value(&metrics, "frame.text.shapes"), 1.0);
    assert_eq!(value(&metrics, "frame.elements"), 30.0);
}

#[test]
fn a_time_settles_on_its_fastest_pass_and_one_slow_pass_adds_no_spread() {
    let counts = MetricsBuilder::default()
        .set("frame.layouts", 1.0)
        .set("ms.cpu", 99.0);
    let passes = [2.0, 2.1, 50.0].map(|time| {
        let pass = MetricsBuilder::default().set("ms.cpu", time);
        pass.set("frame.layouts", 7.0).build()
    });
    let settled = settle(counts.build(), &passes);
    assert_eq!(value(&settled, "ms.cpu"), 2.0);
    assert_eq!(spread(&settled, "ms.cpu"), 0.1);
    assert_eq!(value(&settled, "frame.layouts"), 1.0);
}

#[test]
fn absent_spans_are_not_zero_duration_samples() {
    let timing = MetricsBuilder::default().set("ms.span.paint", 2.0).build();
    let settled = settle(Metrics::new(), &[timing, Metrics::new()]);
    assert_eq!(value(&settled, "ms.span.paint"), 2.0);
    assert_eq!(spread(&settled, "ms.span.paint"), 0.0);
}

#[test]
fn counts_settle_without_times_when_there_are_no_passes() {
    let counts = MetricsBuilder::default()
        .set("frame.layouts", 1.0)
        .set("ms.cpu", 99.0);
    let settled = settle(counts.build(), &[]);
    assert_eq!(
        settled,
        MetricsBuilder::default().set("frame.layouts", 1.0).build()
    );
}

#[test]
fn amounts_stay_short() {
    let shown = [0.25, 2.5, 185.0, 18_276.0, 1_990_000.0, 7.5e9].map(amount);
    assert_eq!(shown, ["0.250", "2.50", "185", "18.3k", "1.99M", "7.50G"]);
}
