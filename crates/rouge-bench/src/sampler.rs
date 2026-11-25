use crate::bench::{MEASURED, Observe, play};
use crate::error::BenchError;
use crate::heap::take_sites;
use crate::scenarios::Case;
use crate::stacks::Stacks;
use backtrace::resolve;
use pprof::{ProfilerGuardBuilder, Symbol};
use rustc_hash::FxHashMap;
use std::ffi::c_void;
use std::time::{Duration, Instant};

const HERTZ: i32 = 997;
const UNWINDER: [&str; 2] = ["libgcc", "vdso"];
const RECORDER: &str = "rouge_bench::heap";
const PATIENCE: u32 = 5;

pub struct Profile {
    pub stacks: Stacks,
    pub frames: usize,
    pub time: Duration,
}

pub struct Allocations {
    pub counts: Stacks,
    pub bytes: Stacks,
    pub frames: usize,
}

pub fn profile(case: &Case, seconds: f64) -> Result<Profile, BenchError> {
    let wanted = Duration::from_secs_f64(seconds.max(0.1));
    let deadline = Instant::now() + wanted * PATIENCE;
    let guard = ProfilerGuardBuilder::default()
        .frequency(HERTZ)
        .blocklist(&UNWINDER)
        .build()
        .map_err(|error| BenchError::Sampler(error.to_string()))?;
    let (mut frames, mut time) = (0, Duration::ZERO);
    while time < wanted && (frames == 0 || Instant::now() < deadline) {
        let (played, spent) = play(case, Observe::Counts, |bench| Ok(bench.measured()))?;
        if played == 0 {
            break;
        }
        frames += played;
        time += spent;
    }
    let report = guard
        .report()
        .build()
        .map_err(|error| BenchError::Sampler(error.to_string()))?;
    drop(guard);
    let mut stacks = Stacks::default();
    for (sampled, count) in &report.data {
        let symbols = sampled.frames.iter().flatten().map(Symbol::name);
        let mut names: Vec<String> = symbols.collect();
        names.reverse();
        stacks.add(names, MEASURED, *count as f64);
    }
    if stacks.total() == 0.0 {
        return Err(BenchError::NoSamples);
    }
    let per_sample = time.as_secs_f64() * 1e3 / frames as f64 / stacks.total();
    Ok(Profile {
        stacks: stacks.scaled(per_sample),
        frames,
        time,
    })
}

pub fn allocations(case: &Case) -> Result<Allocations, BenchError> {
    take_sites();
    let (frames, _) = play(case, Observe::Allocations, |bench| Ok(bench.measured()))?;
    let sites = take_sites();
    if frames == 0 {
        return Err(BenchError::NoSamples);
    }
    let share = 1.0 / frames as f64;
    let mut symbols = Symbols::default();
    let (mut counts, mut bytes) = (Stacks::default(), Stacks::default());
    for (stack, used) in &sites {
        let names = symbols.names(stack);
        counts.add(names.clone(), MEASURED, used.allocations as f64);
        bytes.add(names, MEASURED, used.bytes as f64);
    }
    Ok(Allocations {
        counts: counts.scaled(share),
        bytes: bytes.scaled(share),
        frames,
    })
}

#[derive(Default)]
struct Symbols(FxHashMap<usize, Vec<String>>);

impl Symbols {
    fn names(&mut self, innermost_first: &[usize]) -> Vec<String> {
        let mut names = Vec::new();
        for address in innermost_first {
            let resolved = self.0.entry(*address).or_insert_with(|| {
                let mut found = Vec::new();
                resolve((address - 1) as *mut c_void, |symbol| {
                    found.extend(symbol.name().map(|name| format!("{name:#}")));
                });
                found
            });
            names.extend(resolved.iter().cloned());
        }
        names.reverse();
        let recorder = names.iter().position(|name| name.contains(RECORDER));
        names.truncate(recorder.unwrap_or(names.len()));
        names
    }
}
