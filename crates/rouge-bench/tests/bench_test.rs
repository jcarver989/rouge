#![cfg(not(target_arch = "wasm32"))]

mod common;

use common::TestResult;
use rouge_bench::bench::{Bench, Observe, play};
use rouge_bench::metrics::value;
use rouge_bench::scenarios::Case;

#[test]
fn setup_frames_count_towards_nothing() -> TestResult {
    let case = Case::exact("rows_select@100")?;
    let metrics = play(&case, Observe::Counts, Bench::metrics)?;
    assert_eq!(value(&metrics, "frames"), 25.0);
    assert_eq!(value(&metrics, "frame.elements_created"), 0.0);
    assert_eq!(value(&metrics, "frame.text.shapes"), 0.0);
    assert_eq!(value(&metrics, "frame.layouts"), 0.0);
    assert_eq!(value(&metrics, "frame.patches"), 2.0);
    Ok(())
}

#[test]
fn measured_frames_count_their_own_work() -> TestResult {
    let case = Case::exact("rows_create@1")?;
    let metrics = play(&case, Observe::Counts, Bench::metrics)?;
    assert_eq!(value(&metrics, "frame.elements_created"), 10.0);
    assert!(value(&metrics, "heap.allocations") > 0.0);
    Ok(())
}

#[test]
fn partial_rows_and_odd_mount_sizes_keep_the_remaining_elements() -> TestResult {
    let elements = |name| -> TestResult<f64> {
        let metrics = play(&Case::exact(name)?, Observe::Counts, Bench::metrics)?;
        Ok(value(&metrics, "frame.elements"))
    };
    assert_eq!(elements("mount@3")? - elements("mount@2")?, 1.0);
    assert_eq!(
        elements("layout_nested@7")? - elements("layout_nested@6")?,
        3.0
    );
    Ok(())
}
