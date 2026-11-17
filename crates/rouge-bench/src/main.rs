mod scenarios;

#[cfg(feature = "dhat")]
use dhat::Alloc as Inner;
use rouge::core::testing::{TestError, TestUiBuilder};
use rouge::prelude::*;
use rouge::widgets::bundled_fonts;
use rustc_demangle::demangle;
use scenarios::{SCENARIOS, SIZE, Staged, Step};
use serde_json::{Error as JsonError, from_str, to_string_pretty};
#[cfg(not(feature = "dhat"))]
use std::alloc::System as Inner;
use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::env::args;
use std::fmt::Debug;
use std::fs::{create_dir_all, read_to_string, write};
use std::io::{BufRead, Error as IoError, stdin};
use std::process::ExitCode;
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::Subscriber;
use tracing::span::Id;
use tracing::subscriber::set_default;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

const USAGE: &str = "usage:
  rouge-bench list
  rouge-bench run [filter...] [--runs <n>] [--save]

run     measures the scenarios whose names contain a filter on a headless GPU, writes
        target/bench/latest.json and prints what changed since target/bench/base.json:
        counters that differ, heap numbers that moved by more than 5%, and times that moved
        by more than 5% and more than either report's spread; it fails when one got worse
--runs  timed passes over the scenarios; a time is the fastest pass and its spread is the
        slowest minus the fastest (default 5; 0 measures once, without spans or spreads)
--save  writes target/bench/base.json instead of comparing with it";

const DIR: &str = "target/bench";
const BASE: &str = "target/bench/base.json";
const LATEST: &str = "target/bench/latest.json";
const FRAME: f64 = 1.0 / 120.0;
const TIMED: &str = "ms.";
const SPREAD: &str = ".spread";
const NOISE: f64 = 0.05;
const NOISY: [(&str, f64); 2] = [(TIMED, 0.005), ("heap.", 1.0)];

type Metrics = BTreeMap<String, f64>;
type Report = BTreeMap<String, Metrics>;

thread_local! {
    static HEAP: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

struct Counting;

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { Inner.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { Inner.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { Inner.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { Inner.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[derive(Debug, Error)]
enum BenchError {
    #[error("{}", USAGE)]
    Usage,
    #[error("no scenario matches {0:?}; `rouge-bench list` names them")]
    NoScenario(Vec<String>),
    #[error(transparent)]
    Ui(#[from] TestError),
    #[error("failed to read or write: {0}")]
    Io(#[from] IoError),
    #[error("{BASE} is not a bench report: {0}")]
    Json(#[from] JsonError),
}

struct Spans(Sender<(&'static str, Duration)>);

impl<T: Subscriber + for<'a> LookupSpan<'a>> Layer<T> for Spans {
    fn on_enter(&self, id: &Id, cx: Context<'_, T>) {
        if let Some(span) = cx.span(id) {
            span.extensions_mut().replace(Instant::now());
        }
    }

    fn on_exit(&self, id: &Id, cx: Context<'_, T>) {
        let Some(span) = cx.span(id) else {
            return;
        };
        if let Some(entered) = span.extensions_mut().remove::<Instant>() {
            let _ = self.0.send((span.name(), entered.elapsed()));
        }
    }
}

fn main() -> ExitCode {
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::builder()
        .file_name("target/bench/dhat-heap.json")
        .build();
    let mut arguments = args().skip(1);
    let outcome = match arguments.next().as_deref() {
        Some("list") => list(),
        Some("run") => run(arguments),
        Some("demangle") => readable(),
        _ => Err(BenchError::Usage),
    };
    outcome.unwrap_or_else(|error| {
        eprintln!("rouge-bench: {error}");
        ExitCode::FAILURE
    })
}

fn list() -> Result<ExitCode, BenchError> {
    for (name, _) in SCENARIOS {
        println!("{name}");
    }
    Ok(ExitCode::SUCCESS)
}

fn readable() -> Result<ExitCode, BenchError> {
    for line in stdin().lock().lines() {
        let line = line?;
        let words = line.split(' ').map(|word| format!("{:#}", demangle(word)));
        println!("{}", words.collect::<Vec<String>>().join(" "));
    }
    Ok(ExitCode::SUCCESS)
}

fn run(mut arguments: impl Iterator<Item = String>) -> Result<ExitCode, BenchError> {
    let (mut filters, mut runs, mut save) = (Vec::new(), 5, false);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--save" => save = true,
            "--runs" => {
                let count = arguments.next().and_then(|count| count.parse().ok());
                runs = count.ok_or(BenchError::Usage)?;
            }
            flag if flag.starts_with("--") => return Err(BenchError::Usage),
            _ => filters.push(argument),
        }
    }
    let wanted = |name: &str| filters.is_empty() || filters.iter().any(|part| name.contains(part));
    let base: Report = match read_to_string(BASE) {
        Ok(text) if !save => from_str(&text)?,
        _ => Report::new(),
    };
    let chosen = SCENARIOS.iter().filter(|(name, _)| wanted(name));
    let round = |timed: bool| -> Result<Report, BenchError> {
        let played = chosen
            .clone()
            .map(|(name, build)| Ok(((*name).to_owned(), play(*build, timed)?)));
        played.collect()
    };
    let mut report = round(false)?;
    if report.is_empty() {
        return Err(BenchError::NoScenario(filters));
    }
    eprintln!("measuring {} scenarios {runs} more times", report.len());
    let timed = (0..runs).map(|_| round(true));
    let timed = timed.collect::<Result<Vec<Report>, BenchError>>()?;
    let mut worse = 0;
    println!(
        "{:<18} {:>6} {:>9} {:>9} {:>9} {:>13} {:>8} {:>8}",
        "scenario", "frames", "cpu ms", "p99", "gpu ms", "allocs/frame", "layouts", "paints"
    );
    for (name, metrics) in &mut report {
        let runs: Vec<&Metrics> = timed.iter().filter_map(|round| round.get(name)).collect();
        settle(metrics, &runs);
        let of = |key: &str| metrics.get(key).copied().unwrap_or_default();
        println!(
            "{name:<18} {:>6} {:>9.3} {:>9.3} {:>9.3} {:>13.1} {:>8} {:>8}",
            of("frames"),
            of("ms.cpu"),
            of("ms.cpu.p99"),
            of("ms.gpu"),
            of("heap.allocations"),
            of("frame.layouts"),
            of("frame.layer_paints"),
        );
        for (key, before, after) in changes(base.get(name), metrics) {
            let (percent, verdict) = ((after - before) / before * 100.0, after > before);
            worse += usize::from(verdict);
            let verdict = if verdict { "WORSE" } else { "better" };
            println!("  {key:<30} {before:>14.4} -> {after:>14.4} {percent:>+9.1}%  {verdict}");
        }
    }
    let out = if save { BASE } else { LATEST };
    create_dir_all(DIR)?;
    write(out, to_string_pretty(&report)? + "\n")?;
    eprintln!("wrote {out}");
    if base.is_empty() && !save {
        eprintln!("nothing to compare with; `run --save` writes {BASE}");
    }
    if worse == 0 {
        return Ok(ExitCode::SUCCESS);
    }
    println!("{worse} metrics got worse since {BASE}");
    Ok(ExitCode::FAILURE)
}

fn changes<'a>(
    base: Option<&'a Metrics>,
    new: &'a Metrics,
) -> impl Iterator<Item = (&'a str, f64, f64)> {
    new.iter().filter_map(move |(key, after)| {
        let before = *base?.get(key)?;
        let spread = |run: &Metrics| {
            run.get(&format!("{key}{SPREAD}"))
                .map_or(0.0, |spread| *spread)
        };
        let spread = spread(base?).max(spread(new));
        let floor = NOISY.iter().find(|(prefix, _)| key.starts_with(prefix));
        let noise = floor.map_or(0.0, |(_, floor)| (before * NOISE).max(*floor).max(spread));
        let changed = !key.ends_with(SPREAD) && (after - before).abs() > noise;
        changed.then_some((key.as_str(), before, *after))
    })
}

fn settle(metrics: &mut Metrics, timed: &[&Metrics]) {
    let keys = timed.iter().flat_map(|run| run.keys());
    for key in keys.filter(|key| key.starts_with(TIMED)) {
        let values = timed.iter().filter_map(|run| run.get(key)).copied();
        let least = values.clone().fold(f64::INFINITY, f64::min);
        let most = values.fold(0.0, f64::max);
        metrics.insert(key.clone(), least);
        metrics.insert(format!("{key}{SPREAD}"), most - least);
    }
    for value in metrics.values_mut() {
        *value = (*value * 1e4).round() / 1e4;
    }
}

fn play(scenario: fn() -> Staged, timed: bool) -> Result<Metrics, BenchError> {
    let _entered = Runtime::new().enter();
    let (sender, spans) = channel();
    let _subscribed = timed.then(|| set_default(Registry::default().with(Spans(sender))));
    let mut frames = Vec::with_capacity(1 << 12);
    let (empty, start) = (HEAP.get(), Instant::now());
    let (mut ui, mut script) = TestUiBuilder::default()
        .size(SIZE.width, SIZE.height)
        .scale(2.0)
        .clear(Color::BLACK)
        .fonts(bundled_fonts())
        .build_with(scenario);
    ui.advance(FRAME);
    let (mounted, mount) = (HEAP.get(), start.elapsed());
    ui.draw()?;
    ui.wait()?;

    let mut began = None;
    for frame in 0.. {
        let before = (ui.stats(), HEAP.get());
        let start = Instant::now();
        let step = script(&mut ui, frame);
        if step == Step::Done {
            break;
        }
        if step == Step::Measure && began.is_none() {
            began = Some(before);
            spans.try_iter().for_each(drop);
        }
        ui.advance(FRAME);
        let render = ui.draw()?;
        let (cpu, submitted) = (start.elapsed(), Instant::now());
        ui.wait()?;
        if step == Step::Measure {
            frames.push((cpu, submitted.elapsed(), render));
        }
    }
    let (work, heap) = (ui.stats(), HEAP.get());
    let (began, steady) = began.unwrap_or((work, heap));

    let per_frame = |total: u64| total as f64 / frames.len().max(1) as f64;
    let cpu = |p: f64| percentile(frames.iter().map(|frame| frame.0), p);
    let mut metrics: Metrics = [
        ("frames", frames.len() as f64),
        ("heap.allocations", per_frame(heap.0 - steady.0)),
        ("heap.bytes", per_frame(heap.1 - steady.1)),
        ("heap.build.allocations", (mounted.0 - empty.0) as f64),
        ("heap.build.bytes", (mounted.1 - empty.1) as f64),
        ("ms.build", millis(mount)),
        ("ms.cpu", cpu(0.5)),
        ("ms.cpu.p99", cpu(0.99)),
        (
            "ms.gpu",
            percentile(frames.iter().map(|frame| frame.1), 0.5),
        ),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value))
    .collect();
    scrape(&mut metrics, "frame", &(work - began));
    for (.., render) in &frames {
        scrape(&mut metrics, "render", render);
    }
    scrape(&mut metrics, "memory", &ui.memory()?);
    for (name, time) in spans.try_iter() {
        *metrics.entry(format!("ms.span.{name}")).or_default() += per_frame(1) * millis(time);
    }
    Ok(metrics)
}

fn scrape(metrics: &mut Metrics, root: &str, stats: &impl Debug) {
    let text = format!("{stats:?}");
    let (mut path, mut field) = (Vec::new(), root);
    for token in text.split_whitespace() {
        let token = token.trim_end_matches(',');
        if token == "{" {
            path.push(field);
        } else if token == "}" {
            path.pop();
        } else if let Some(name) = token.strip_suffix(':') {
            field = name;
        } else if let Ok(value) = token.parse::<u64>() {
            *metrics
                .entry(format!("{}.{field}", path.join(".")))
                .or_default() += value as f64;
        }
    }
}

fn percentile(times: impl Iterator<Item = Duration>, p: f64) -> f64 {
    let mut sorted: Vec<Duration> = times.collect();
    sorted.sort();
    let index = ((sorted.len() as f64 * p) as usize).min(sorted.len().saturating_sub(1));
    sorted.get(index).copied().map_or(0.0, millis)
}

fn millis(time: Duration) -> f64 {
    time.as_secs_f64() * 1e3
}

fn count(size: usize) {
    let _ = HEAP.try_with(|heap| {
        let (allocations, bytes) = heap.get();
        heap.set((allocations + 1, bytes + size as u64));
    });
}
