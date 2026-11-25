use crate::builds::Build;
use crate::error::BenchError;
use crate::metrics::{Metrics, Report, settle};
use crate::scenarios::Case;
use serde_json::from_slice;
use std::process::Command;

pub const MISSING: u8 = 3;

pub fn measure(
    builds: &[Build],
    cases: &[Case],
    passes: usize,
    mut progress: impl FnMut(&Case, usize),
) -> Result<Vec<Report>, BenchError> {
    let mut reports = vec![Report::new(); builds.len()];
    for (index, case) in cases.iter().enumerate() {
        progress(case, index);
        let mut counts = Vec::with_capacity(builds.len());
        for build in builds {
            counts.push(sample(build, case, false)?);
        }
        let mut timed = vec![Vec::new(); builds.len()];
        for pass in 0..passes {
            for turn in 0..builds.len() {
                let which = (pass + turn) % builds.len();
                if counts[which].is_some() {
                    timed[which].extend(sample(&builds[which], case, true)?);
                }
            }
        }
        for ((report, counts), timed) in reports.iter_mut().zip(counts).zip(timed) {
            if let Some(counts) = counts {
                report.insert(case.to_string(), settle(counts, &timed));
            }
        }
    }
    Ok(reports)
}

fn sample(build: &Build, case: &Case, timed: bool) -> Result<Option<Metrics>, BenchError> {
    let mut command = Command::new(&build.binary);
    command.arg("sample").arg(case.to_string());
    if timed {
        command.arg("--timed");
    }
    let output = command.output()?;
    if output.status.code() == Some(i32::from(MISSING)) {
        return Ok(None);
    }
    if !output.status.success() {
        return Err(BenchError::Pass {
            build: build.label.clone(),
            case: case.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(Some(from_slice(&output.stdout)?))
}
