#![cfg(not(target_arch = "wasm32"))]

mod common;

use common::TestResult;
use rouge_bench::error::BenchError;
use rouge_bench::scenarios::{Case, SCENARIOS};
use std::collections::HashSet;

#[test]
fn every_scenario_has_its_own_name() {
    let names: HashSet<&str> = SCENARIOS.iter().map(|scenario| scenario.name).collect();
    assert_eq!(names.len(), SCENARIOS.len());
}

#[test]
fn every_case_can_be_named_exactly() -> TestResult {
    for case in Case::all() {
        let name = case.to_string();
        assert_eq!(Case::exact(&name)?.to_string(), name);
    }
    Ok(())
}

#[test]
fn a_filter_matches_parts_of_names_at_their_sizes() -> TestResult {
    assert_eq!(
        matching(&["rows_create"])?,
        ["rows_create@1000", "rows_create@10000"]
    );
    assert_eq!(
        matching(&["editor_caret", "caret_access"])?,
        ["editor_caret@2000", "editor_caret_access@2000"]
    );
    Ok(())
}

#[test]
fn a_size_replaces_the_listed_ones() -> TestResult {
    assert_eq!(matching(&["rows_create@5_000"])?, ["rows_create@5000"]);
    assert!(matches!(
        Case::exact("gallery@5"),
        Err(BenchError::Unsized(_))
    ));
    assert!(matches!(
        Case::exact("rows_swap@many"),
        Err(BenchError::BadSize(_))
    ));
    Ok(())
}

#[test]
fn sizes_that_cannot_exercise_an_operation_are_rejected() -> TestResult {
    for (name, small, minimum) in [
        ("rows_select", 4, 5),
        ("rows_remove", 3, 4),
        ("rows_swap", 3, 4),
        ("keyed_prepend", 8, 50),
    ] {
        let case = format!("{name}@{small}");
        for result in [Case::exact(&case), Case::exact(name)?.resized(small)] {
            assert!(
                matches!(result, Err(BenchError::BelowMinimum { minimum: n, .. }) if n == minimum)
            );
        }
        assert!(matches!(
            matching(&[&case]),
            Err(BenchError::BelowMinimum { .. })
        ));
        Case::exact(&format!("{name}@{minimum}"))?;
    }
    Ok(())
}

#[test]
fn a_single_case_prefers_an_exact_name() -> TestResult {
    let case = Case::single("editor_caret")?;
    assert_eq!(case.to_string(), "editor_caret@2000");
    assert!(matches!(
        Case::single("rows"),
        Err(BenchError::Ambiguous(9, _))
    ));
    assert!(matches!(
        Case::single("nope"),
        Err(BenchError::NoScenario(_))
    ));
    Ok(())
}

fn matching(filters: &[&str]) -> Result<Vec<String>, BenchError> {
    let filters: Vec<String> = filters.iter().map(|&filter| filter.to_owned()).collect();
    let cases = Case::matching(&filters)?;
    Ok(cases.iter().map(Case::to_string).collect())
}
