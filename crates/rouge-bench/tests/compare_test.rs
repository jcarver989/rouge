#![cfg(not(target_arch = "wasm32"))]
mod common;
use common::TestResult;
use serde_json::{Value, json, to_vec};
use std::fs::write;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn reports_must_compare_the_same_work_before_scoring_it() -> TestResult {
    let dir = tempdir()?;
    let base = dir.path().join("base.json");
    let report = json!({"schema":1,"environment":"test", "passes":1,"cases":{"a":[{"frames":1,"ms.cpu":1.0,"ms.gpu_wait":0.3,"heap.allocations":2,"frame.layouts":1,"render.instance_bytes_uploaded":3,"memory.bytes":4}]}});
    write(&base, to_vec(&report)?)?;
    let check = |after: &Value, names: &[&str]| -> TestResult<(bool, String)> {
        let candidate = dir.path().join("after.json");
        write(&candidate, to_vec(after)?)?;
        let result = Command::new(env!("CARGO_BIN_EXE_rouge-bench"))
            .arg("compare")
            .arg(&base)
            .arg(candidate)
            .args(names)
            .output()?;
        Ok((result.status.success(), String::from_utf8(result.stderr)?))
    };
    assert!(check(&report, &[])?.0);
    assert!(!check(&report, &["missing"])?.0);
    for (path, value) in [
        ("/environment", json!("other")),
        ("/schema", json!(0)),
        ("/cases", json!({})),
        ("/cases/a", json!([])),
        ("/cases/a/0/frames", json!(2)),
    ] {
        let mut after = report.clone();
        *after.pointer_mut(path).ok_or("bad fixture path")? = value;
        assert!(!check(&after, &[])?.0);
    }
    for (key, value, same) in [
        ("ms.gpu_wait", json!(9.0), true),
        ("ms.cpu", json!(0.5), true),
        ("heap.allocations", json!(2.05), true),
        ("heap.allocations", json!(3), false),
        ("render.instance_bytes_uploaded", json!(2), false),
        ("frame.layouts", json!(2), false),
        ("memory.bytes", json!(5), false),
    ] {
        let mut after = report.clone();
        after["cases"]["a"][0][key] = value;
        assert_eq!(check(&after, &[])?.0, same, "{key}");
    }
    let mut worse = report.clone();
    worse["cases"]["a"][0]["ms.cpu"] = json!(1.5);
    assert!(!check(&worse, &[])?.0);
    worse["cases"]["a"][0]
        .as_object_mut()
        .ok_or("missing sample")?
        .remove("ms.cpu");
    assert!(check(&worse, &[])?.1.contains("ms.cpu"));
    Ok(())
}
