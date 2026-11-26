#![cfg(not(target_arch = "wasm32"))]

mod common;
use common::TestResult;
use rouge::core::testing::{TestUi, TestUiBuilder};
use rouge::prelude::*;
use rouge::widgets::bundled_fonts;
use rouge_bench::scenarios::{Case, row_table};

#[test]
fn named_cases_round_trip_and_sizes_must_exercise_the_operation() -> TestResult {
    for case in Case::all() {
        assert_eq!(
            Case::exact(&case.to_string())?.to_string(),
            case.to_string()
        );
    }
    for bad in [
        "rows_swap@3",
        "rows_remove@3",
        "rows_select@4",
        "gallery@2",
        "rows_create@0",
        "missing",
    ] {
        assert!(Case::exact(bad).is_err(), "{bad}");
    }
    Ok(())
}

#[test]
fn table_clicks_preserve_row_contents_and_key_order() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default()
        .size(800.0, 1100.0)
        .scale(1.0)
        .fonts(bundled_fonts())
        .build(|| row_table(12));
    ui.click_text("Create")?;
    let created = rows(&mut ui)?;
    assert_eq!(
        created.iter().map(|r| r.0).collect::<Vec<_>>(),
        (1..=12).collect::<Vec<_>>()
    );
    ui.click_text("Update")?;
    let mut expected = created;
    for (_, label) in expected.iter_mut().step_by(10) {
        label.push_str(" !!!");
    }
    assert_eq!(rows(&mut ui)?, expected);
    ui.click(Point::new(100.0, 102.0));
    ui.advance(0.0);
    assert_eq!(ui.pixel(610, 102)?, [0x45, 0x47, 0x5a, 255]);
    ui.click_text("Swap")?;
    expected.swap(1, 10);
    assert_eq!(rows(&mut ui)?, expected);
    ui.click(Point::new(440.0, 174.0));
    expected.remove(3);
    assert_eq!(rows(&mut ui)?, expected);
    ui.click_text("Append")?;
    let appended = rows(&mut ui)?;
    assert_eq!(&appended[..11], expected);
    assert_eq!(
        appended[11..].iter().map(|r| r.0).collect::<Vec<_>>(),
        (13..=24).collect::<Vec<_>>()
    );
    ui.click_text("Create")?;
    assert_eq!(
        rows(&mut ui)?.iter().map(|r| r.0).collect::<Vec<_>>(),
        (25..=36).collect::<Vec<_>>()
    );
    ui.click_text("Clear")?;
    assert_eq!(rows(&mut ui)?.len(), 0);
    Ok(())
}

fn rows(ui: &mut TestUi) -> TestResult<Vec<(usize, String)>> {
    let values: Vec<_> = ui
        .access_nodes()
        .into_iter()
        .filter_map(|n| n.value)
        .collect();
    let body: Vec<_> = values
        .into_iter()
        .skip_while(|s| s.parse::<usize>().is_err())
        .collect();
    let (rows, rest) = body.as_chunks::<3>();
    assert_eq!(rest.len(), 0);
    rows.iter()
        .map(|[id, label, remove]| {
            assert_eq!(remove, "x");
            Ok((id.parse()?, label.clone()))
        })
        .collect()
}
