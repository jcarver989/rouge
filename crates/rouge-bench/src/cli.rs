use crate::compare::{Report, SCHEMA};
use clap::{Parser, Subcommand};
use rouge::core::testing::TestError;
use rouge::render::headless::headless_gpu;
use rouge_bench::bench::play;
use rouge_bench::error::BenchError;
use rouge_bench::scenarios::Case;
use serde_json::{from_slice, to_string_pretty};
use std::collections::BTreeMap;
use std::env::{
    consts::{ARCH, OS},
    current_exe,
};
use std::fs::{create_dir_all, read, write};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use tracing_subscriber::fmt::format::FmtSpan;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand)]
enum Task {
    List,
    Run {
        filters: Vec<String>,
        #[arg(long, default_value = "3")]
        passes: NonZeroUsize,
        #[arg(long, default_value = "target/bench/latest.json")]
        output: PathBuf,
    },
    Compare {
        base: PathBuf,
        candidate: PathBuf,
        cases: Vec<String>,
    },
    Sample {
        case: String,
        #[arg(long)]
        repeat: bool,
        #[arg(long)]
        trace: bool,
    },
}

pub(crate) fn main() -> Result<ExitCode, BenchError> {
    match Cli::parse().task {
        Task::List => {
            for case in Case::all() {
                println!("{case}");
            }
        }
        Task::Run {
            filters,
            passes,
            output,
        } => {
            let cases = Case::matching(&filters)?;
            let info = headless_gpu().map_err(TestError::from)?.adapter.get_info();
            let profile = if cfg!(debug_assertions) {
                "debug"
            } else {
                "optimized"
            };
            let environment = format!(
                "{OS}/{ARCH} {} {:?} {} {profile} dhat={}",
                info.name,
                info.backend,
                info.driver_info,
                cfg!(feature = "dhat")
            );
            let mut report = Report {
                schema: SCHEMA,
                environment,
                passes: passes.get(),
                cases: BTreeMap::new(),
            };
            let exe = current_exe()?;
            for _ in 0..passes.get() {
                for case in &cases {
                    let name = case.to_string();
                    let child = Command::new(&exe)
                        .args(["sample", &name])
                        .stderr(Stdio::inherit())
                        .output()?;
                    if !child.status.success() {
                        return Err(BenchError::Worker(name));
                    }
                    report
                        .cases
                        .entry(name)
                        .or_default()
                        .push(from_slice(&child.stdout)?);
                }
            }
            for case in report.cases.keys() {
                report.summary(case)?;
            }
            if let Some(parent) = output.parent() {
                create_dir_all(parent)?;
            }
            write(&output, to_string_pretty(&report)?)?;
            eprintln!("wrote {}", output.display());
        }
        Task::Compare {
            base,
            candidate,
            cases,
        } => {
            let base: Report = from_slice(&read(base)?)?;
            if base.compare(&from_slice(&read(candidate)?)?, &cases)? {
                return Ok(ExitCode::FAILURE);
            }
        }
        Task::Sample {
            case,
            trace,
            repeat,
        } => {
            let case = Case::exact(&case)?;
            if trace {
                tracing_subscriber::fmt()
                    .with_max_level(tracing_subscriber::filter::LevelFilter::TRACE)
                    .with_span_events(FmtSpan::CLOSE)
                    .with_writer(std::io::stderr)
                    .init();
            }
            #[cfg(feature = "dhat")]
            let _profiler = {
                create_dir_all("target/bench")?;
                dhat::Profiler::builder()
                    .file_name("target/bench/dhat-heap.json")
                    .build()
            };
            loop {
                let metrics = play(&case)?;
                if !repeat {
                    println!("{}", to_string_pretty(&metrics)?);
                    break;
                }
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}
