#![cfg(not(target_arch = "wasm32"))]

mod common;

use common::TestResult;
use rouge_bench::metrics::{Metrics, value};
use rouge_bench::passes::MISSING;
use serde_json::from_slice;
use std::process::{Command, Output};

#[test]
fn a_timed_sample_reports_spans_and_what_is_outside_them() -> TestResult {
    let output = rouge_bench(&["sample", "rows_create@100", "--timed"])?;
    let metrics: Metrics = from_slice(&output.stdout)?;
    for key in [
        "ms.cpu",
        "ms.outside",
        "ms.span.update",
        "ms.span.layout",
        "ms.span.paint",
    ] {
        assert!(value(&metrics, key) > 0.0, "{key}");
    }
    assert!(value(&metrics, "ms.span.update") >= value(&metrics, "ms.span.layout"));
    Ok(())
}

#[test]
fn an_untimed_sample_reports_no_spans() -> TestResult {
    let output = rouge_bench(&["sample", "rows_select@100"])?;
    let metrics: Metrics = from_slice(&output.stdout)?;
    assert!(metrics.keys().all(|key| !key.starts_with("ms.span.")));
    assert_eq!(value(&metrics, "frames"), 25.0);
    Ok(())
}

#[test]
fn a_build_without_the_scenario_says_so() -> TestResult {
    let output = rouge_bench(&["sample", "nope"])?;
    assert_eq!(output.status.code(), Some(i32::from(MISSING)));
    Ok(())
}

#[test]
fn the_list_shows_sizes() -> TestResult {
    let output = rouge_bench(&["list"])?;
    let list = String::from_utf8(output.stdout)?;
    let rows = list.lines().find(|line| line.starts_with("rows_create "));
    assert!(rows.is_some_and(|line| line.ends_with("@1000 @10000")));
    Ok(())
}

#[test]
fn mistakes_are_explained() -> TestResult {
    for (arguments, reason) in [
        (["shot", "nope"], "no scenario matches"),
        (["shot", "gallery@5"], "has no size"),
        (["run", "rows_swap@x"], "is not a size"),
        (["save", "../x"], "is not a name for a saved build"),
    ] {
        let output = rouge_bench(&arguments)?;
        let error = String::from_utf8(output.stderr)?;
        assert!(!output.status.success(), "{arguments:?}");
        assert!(error.contains(reason), "{arguments:?}: {error}");
    }
    Ok(())
}

fn rouge_bench(arguments: &[&str]) -> TestResult<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rouge-bench"));
    Ok(command.args(arguments).output()?)
}
