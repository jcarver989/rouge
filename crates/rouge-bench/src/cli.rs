use clap::{Parser, Subcommand};
use rouge_bench::bench::{Bench, Observe, play};
use rouge_bench::builds::{Build, Builds};
use rouge_bench::compare::{Change, Verdict, changes, percent};
use rouge_bench::error::BenchError;
use rouge_bench::metrics::{CPU, Metrics, Report, SPREAD, TIMED, amount, spread, value};
use rouge_bench::passes::{MISSING, measure};
use rouge_bench::scale::{growth, sizes};
use rouge_bench::scenarios::{Case, SCENARIOS};
#[cfg(all(unix, feature = "profile"))]
use rouge_bench::stacks::Stacks;
use serde_json::to_string_pretty;
use std::fs::{create_dir_all, write};
use std::path::PathBuf;
use std::process::ExitCode;

const SHOWN: usize = 8;
const FAILED: u8 = 1;
const USAGE: u8 = 2;

#[derive(Parser)]
#[command(about = "Measure, compare and profile Rouge on a headless GPU")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    #[command(about = "List the scenarios and the sizes they run at")]
    List,
    #[command(about = "Measure the working tree")]
    Run {
        #[arg(help = "Parts of scenario names; add `@<size>` to choose a size")]
        scenarios: Vec<String>,
        #[arg(long, default_value_t = 5, help = "Timed passes; 0 only counts work")]
        passes: usize,
    },
    #[command(about = "Measure the working tree against a git revision or a saved build")]
    Compare {
        #[arg(help = "Parts of scenario names; add `@<size>` to choose a size")]
        scenarios: Vec<String>,
        #[arg(
            long,
            help = "Revision or saved build to compare with [default: where HEAD left main]"
        )]
        base: Option<String>,
        #[arg(
            long = "with",
            help = "Compare this revision or saved build instead of the working tree; repeatable"
        )]
        candidates: Vec<String>,
        #[arg(long, default_value_t = 5, help = "Timed passes of each build")]
        passes: usize,
    },
    #[command(about = "Keep the working tree's build under a name to compare with later")]
    Save { name: String },
    #[command(about = "Measure scenarios at several sizes and report what grows too fast")]
    Scale {
        #[arg(required = true, help = "Parts of scenario names")]
        scenarios: Vec<String>,
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "1000,3000,10000",
            help = "Sizes to measure"
        )]
        sizes: Vec<usize>,
        #[arg(long, default_value_t = 3, help = "Timed passes of each size")]
        passes: usize,
    },
    #[command(about = "Show where the CPU time of the measured frames goes")]
    Profile {
        scenario: String,
        #[arg(long, default_value_t = 5.0, help = "Measured time to sample")]
        seconds: f64,
        #[arg(
            long,
            help = "Only show what runs under functions with this in their name"
        )]
        under: Option<String>,
        #[arg(
            long,
            default_value_t = 2.0,
            help = "Hide functions below this percentage"
        )]
        min: f64,
    },
    #[command(about = "Show where the allocations of the measured frames come from")]
    Heap {
        scenario: String,
        #[arg(
            long,
            help = "Only show what runs under functions with this in their name"
        )]
        under: Option<String>,
        #[arg(
            long,
            default_value_t = 2.0,
            help = "Hide functions below this percentage"
        )]
        min: f64,
    },
    #[command(about = "Save a screenshot of a scenario's last frame")]
    Shot { scenario: String },
    #[command(hide = true)]
    Sample {
        case: String,
        #[arg(long)]
        timed: bool,
    },
}

pub(crate) fn main() -> ExitCode {
    let outcome = match Cli::parse().command {
        Task::List => list(),
        Task::Run { scenarios, passes } => run(&scenarios, passes),
        Task::Compare {
            scenarios,
            base,
            candidates,
            passes,
        } => compare(&scenarios, base, &candidates, passes),
        Task::Save { name } => save(&name),
        Task::Scale {
            scenarios,
            sizes,
            passes,
        } => scale(&scenarios, &sizes, passes),
        Task::Profile {
            scenario,
            seconds,
            under,
            min,
        } => profile(&scenario, seconds, under.as_deref(), min),
        Task::Heap {
            scenario,
            under,
            min,
        } => heap(&scenario, under.as_deref(), min),
        Task::Shot { scenario } => shot(&scenario),
        Task::Sample { case, timed } => sample(&case, timed),
    };
    outcome.unwrap_or_else(|error| {
        eprintln!("rouge-bench: {error}");
        ExitCode::from(USAGE)
    })
}

fn list() -> Result<ExitCode, BenchError> {
    for scenario in SCENARIOS {
        let sizes = scenario.sizes.iter().map(|size| format!("@{size}"));
        println!(
            "{:<24} {}",
            scenario.name,
            sizes.collect::<Vec<String>>().join(" ")
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn run(filters: &[String], passes: usize) -> Result<ExitCode, BenchError> {
    let builds = Builds::open()?;
    let cases = Case::matching(filters)?;
    let reports = measure(&[builds.current()?], &cases, passes, progress(cases.len()))?;
    let report = reports.into_iter().next().unwrap_or_default();
    summary(&report);
    if let [case] = cases[..]
        && let Some(metrics) = report.get(&case.to_string())
    {
        details(metrics);
    }
    let out = builds.out()?.join("latest.json");
    write(&out, to_string_pretty(&report)? + "\n")?;
    eprintln!("wrote {}", out.display());
    Ok(ExitCode::SUCCESS)
}

fn compare(
    filters: &[String],
    base: Option<String>,
    candidates: &[String],
    passes: usize,
) -> Result<ExitCode, BenchError> {
    let builds = Builds::open()?;
    let cases = Case::matching(filters)?;
    let base = match base {
        Some(base) => builds.resolve(&base)?,
        None => builds.base()?,
    };
    let mut compared = vec![base];
    for candidate in candidates {
        compared.push(builds.resolve(candidate)?);
    }
    if candidates.is_empty() {
        compared.push(builds.current()?);
    }
    let passes = passes.max(1);
    let reports = measure(&compared, &cases, passes, progress(cases.len()))?;
    let Some((before, after)) = reports.split_first() else {
        return Ok(ExitCode::SUCCESS);
    };
    let mut worse = false;
    for (build, report) in compared[1..].iter().zip(after) {
        println!(
            "\n{} -> {}, {passes} interleaved passes",
            compared[0].label, build.label
        );
        worse |= contrast(&cases, (&compared[0], before), (build, report));
    }
    Ok(ExitCode::from(if worse { FAILED } else { 0 }))
}

fn contrast(cases: &[Case], base: (&Build, &Report), candidate: (&Build, &Report)) -> bool {
    let (mut better, mut worse, mut same) = (0, 0, 0);
    for case in cases.iter().map(Case::to_string) {
        let (Some(before), Some(after)) = (base.1.get(&case), candidate.1.get(&case)) else {
            let missing = if base.1.contains_key(&case) {
                candidate.0
            } else {
                base.0
            };
            println!("{case:<28} skipped: {} does not have it", missing.label);
            continue;
        };
        let changes = changes(before, after);
        let is = |verdict| changes.iter().any(|change| change.verdict == verdict);
        let mark = match (is(Verdict::Worse), is(Verdict::Better)) {
            (true, true) => "mixed",
            (true, false) => "WORSE",
            (false, true) => "better",
            (false, false) => "",
        };
        worse += usize::from(is(Verdict::Worse));
        better += usize::from(is(Verdict::Better) && !is(Verdict::Worse));
        same += usize::from(mark.is_empty());
        let (old, new) = (value(before, CPU), value(after, CPU));
        println!(
            "{case:<28} cpu ms {old:>9.3} -> {new:>9.3} {:>8}  {mark}",
            percent(old, new)
        );
        let listed = |change: &&Change| change.key != CPU;
        for change in changes.iter().filter(listed).take(SHOWN) {
            println!("    {change}");
        }
        let hidden = changes.iter().filter(listed).count().saturating_sub(SHOWN);
        if hidden > 0 {
            println!("    and {hidden} more");
        }
    }
    println!("{better} better, {worse} with something worse, {same} unchanged");
    worse > 0
}

fn save(name: &str) -> Result<ExitCode, BenchError> {
    let build = Builds::open()?.save(name)?;
    println!("saved {}; `compare --base {name}` uses it", build.label);
    Ok(ExitCode::SUCCESS)
}

fn scale(filters: &[String], requested: &[usize], passes: usize) -> Result<ExitCode, BenchError> {
    let builds = Builds::open()?;
    let sizes = sizes(requested)?;
    let mut scaled: Vec<Case> = Vec::new();
    for case in Case::matching(filters)? {
        if case.size().is_some() && scaled.iter().all(|known| known.name() != case.name()) {
            scaled.push(case);
        }
    }
    if scaled.is_empty() {
        return Err(BenchError::Unsized(filters.join(" ")));
    }
    let mut cases = Vec::new();
    for case in &scaled {
        for size in &sizes {
            cases.push(case.resized(*size)?);
        }
    }
    let passes = passes.max(1);
    let reports = measure(&[builds.current()?], &cases, passes, progress(cases.len()))?;
    let report = reports.into_iter().next().unwrap_or_default();
    for case in &scaled {
        let mut points: Vec<(usize, Metrics)> = Vec::new();
        for size in &sizes {
            let resized = case.resized(*size)?.to_string();
            if let Some(metrics) = report.get(&resized) {
                points.push((*size, metrics.clone()));
            }
        }
        let header = sizes.iter().map(|size| format!("{size:>10}"));
        println!(
            "\n{:<34}{}   growth",
            case.name(),
            header.collect::<String>()
        );
        let growth = growth(&points);
        let shown = growth
            .iter()
            .filter(|growth| growth.key == CPU || growth.is_superlinear());
        for growth in shown {
            let values = growth.values.iter().map(|v| format!("{:>10}", amount(*v)));
            println!(
                "  {:<32}{}   n^{:.2}{}",
                growth.key,
                values.collect::<String>(),
                growth.exponent,
                if growth.is_superlinear() {
                    "  SUPERLINEAR"
                } else {
                    ""
                }
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(all(unix, feature = "profile"))]
fn profile(
    scenario: &str,
    seconds: f64,
    under: Option<&str>,
    min: f64,
) -> Result<ExitCode, BenchError> {
    use rouge_bench::sampler::profile;
    let case = Case::single(scenario)?;
    let profile = profile(&case, seconds)?;
    println!(
        "{case}: {} frames, {:.3} ms of CPU per frame, sampled for {:.1} s",
        profile.frames,
        profile.stacks.total(),
        profile.time.as_secs_f64()
    );
    if profile.time.as_secs_f64() < seconds {
        eprintln!(
            "stopped early because unmeasured setup takes most of the time; raise --seconds for more samples"
        );
    }
    let out = report_stacks(&case, "profile", &profile.stacks, under, min, "ms")?;
    eprintln!(
        "wrote {}; open it at https://www.speedscope.app",
        out.display()
    );
    Ok(ExitCode::SUCCESS)
}

#[cfg(all(unix, feature = "profile"))]
fn heap(scenario: &str, under: Option<&str>, min: f64) -> Result<ExitCode, BenchError> {
    use rouge_bench::sampler::allocations;
    let case = Case::single(scenario)?;
    let allocations = allocations(&case)?;
    println!(
        "{case}: {} frames, {} allocations and {} bytes per frame",
        allocations.frames,
        amount(allocations.counts.total()),
        amount(allocations.bytes.total())
    );
    let counts = &allocations.counts;
    let out = report_stacks(&case, "heap", counts, under, min, "allocations")?;
    eprintln!(
        "wrote {}; open it at https://www.speedscope.app",
        out.display()
    );
    Ok(ExitCode::SUCCESS)
}

#[cfg(all(unix, feature = "profile"))]
fn report_stacks(
    case: &Case,
    kind: &str,
    stacks: &Stacks,
    under: Option<&str>,
    min: f64,
    unit: &str,
) -> Result<PathBuf, BenchError> {
    const OWN: usize = 15;
    const FINE: f64 = 1000.0;
    let whole = stacks.total();
    let narrowed = under.map(|part| stacks.under(part));
    let shown = narrowed.as_ref().unwrap_or(stacks);
    if shown.total() == 0.0 {
        println!("nothing ran under {:?}", under.unwrap_or_default());
    }
    let cost = |weight: f64| {
        format!(
            "{:>5.1}% {:>9}",
            weight / whole.max(f64::MIN_POSITIVE) * 100.0,
            amount(weight)
        )
    };
    println!("\n  share {unit:>11}  calls, with everything they call");
    print!("{}", shown.tree(min / 100.0, cost));
    println!("\n  share {unit:>11}  functions, on their own");
    for (name, weight) in shown.own().iter().take(OWN) {
        println!("{}  {name}", cost(*weight));
    }
    let out = Builds::open()?.out()?.join(format!("{kind}-{case}.folded"));
    let fine = if kind == "profile" { FINE } else { 1.0 };
    write(&out, shown.clone().scaled(fine).folded())?;
    Ok(out)
}

#[cfg(not(all(unix, feature = "profile")))]
fn profile(_: &str, _: f64, _: Option<&str>, _: f64) -> Result<ExitCode, BenchError> {
    Err(BenchError::NoProfiler)
}

#[cfg(not(all(unix, feature = "profile")))]
fn heap(_: &str, _: Option<&str>, _: f64) -> Result<ExitCode, BenchError> {
    Err(BenchError::NoProfiler)
}

fn shot(scenario: &str) -> Result<ExitCode, BenchError> {
    let case = Case::single(scenario)?;
    let image = play(&case, Observe::Counts, |bench| Ok(bench.ui()?.render()?))?;
    let shots = Builds::open()?.out()?.join("shots");
    create_dir_all(&shots)?;
    let out: PathBuf = shots.join(format!("{case}.png"));
    write(&out, image.to_png()?)?;
    println!("{}", out.display());
    Ok(ExitCode::SUCCESS)
}

fn sample(case: &str, timed: bool) -> Result<ExitCode, BenchError> {
    let case = match Case::exact(case) {
        Ok(case) => case,
        Err(BenchError::NoScenario(_) | BenchError::Unsized(_)) => {
            return Ok(ExitCode::from(MISSING));
        }
        Err(error) => return Err(error),
    };
    let observe = if timed {
        Observe::Times
    } else {
        Observe::Counts
    };
    let metrics = play(&case, observe, Bench::metrics)?;
    println!("{}", to_string_pretty(&metrics)?);
    Ok(ExitCode::SUCCESS)
}

fn progress(total: usize) -> impl FnMut(&Case, usize) {
    move |case, index| eprintln!("[{}/{total}] {case}", index + 1)
}

fn summary(report: &Report) {
    println!(
        "{:<28} {:>6} {:>9} {:>7} {:>8} {:>8} {:>8} {:>7} {:>9} {:>9}",
        "per frame",
        "frames",
        "cpu ms",
        "±",
        "layout",
        "paint",
        "outside",
        "gpu",
        "allocs",
        "upload"
    );
    for (name, metrics) in report {
        let of = |key: &str| value(metrics, key);
        let time = |key: &str| {
            let time = metrics.get(key);
            time.map_or_else(|| "-".to_owned(), |time| format!("{time:.3}"))
        };
        println!(
            "{name:<28} {:>6} {:>9} {:>7} {:>8} {:>8} {:>8} {:>7} {:>9} {:>9}",
            of("frames"),
            time(CPU),
            time(&format!("{CPU}{SPREAD}")),
            time("ms.span.layout"),
            time("ms.span.paint"),
            time("ms.outside"),
            time("ms.gpu"),
            amount(of("heap.allocations")),
            amount(of("render.instance_bytes_uploaded")),
        );
    }
}

fn details(metrics: &Metrics) {
    println!();
    let shown = metrics
        .iter()
        .filter(|(key, value)| !key.ends_with(SPREAD) && **value != 0.0);
    for (key, value) in shown {
        let noise = if key.starts_with(TIMED) {
            format!("  ± {}", amount(spread(metrics, key)))
        } else {
            String::new()
        };
        println!("  {key:<34} {:>10}{noise}", amount(*value));
    }
}
