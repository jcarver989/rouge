use rouge_core::{FrameStats, Ui};
use rouge_render::Point;
use std::env::var;
use std::time::{Duration, Instant};
use tracing::level_filters::LevelFilter;
use tracing::subscriber::set_global_default;
use tracing::trace;
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::registry;
use wgpu::PresentMode;

const BUDGET: Duration = Duration::from_micros(16_667);

pub(crate) fn trace() -> Option<FlushGuard> {
    let path = var("ROUGE_PROFILE").ok().filter(|path| !path.is_empty())?;
    let (layer, guard) = ChromeLayerBuilder::new()
        .file(&path)
        .include_args(true)
        .build();
    let layer = layer.with_filter(
        Targets::new()
            .with_target("winit", LevelFilter::OFF)
            .with_default(LevelFilter::TRACE),
    );
    if set_global_default(registry().with(layer)).is_err() {
        eprintln!("rouge: ROUGE_PROFILE is ignored because a tracing subscriber is already set");
        return None;
    }
    eprintln!("rouge profile: tracing to {path}");
    Some(guard)
}

pub(crate) fn present_mode() -> Option<PresentMode> {
    let mode = var("ROUGE_PRESENT_MODE").ok()?;
    match mode.to_ascii_lowercase().as_str() {
        "fifo" => Some(PresentMode::Fifo),
        "fifo-relaxed" => Some(PresentMode::FifoRelaxed),
        "mailbox" => Some(PresentMode::Mailbox),
        "immediate" => Some(PresentMode::Immediate),
        "auto-vsync" => Some(PresentMode::AutoVsync),
        "auto-no-vsync" => Some(PresentMode::AutoNoVsync),
        _ => {
            eprintln!("rouge: unknown ROUGE_PRESENT_MODE `{mode}`");
            None
        }
    }
}

struct Bench {
    pixels: f32,
    remaining: u32,
    direction: f32,
}

pub(crate) struct Profile {
    environment: String,
    bench: Option<Bench>,
    intervals: Vec<Duration>,
    last: Option<Instant>,
    counting: bool,
    previous: FrameStats,
    first: FrameStats,
}

impl Profile {
    pub(crate) fn from_env(ui: &mut Ui, environment: String) -> Option<Profile> {
        let bench = var("ROUGE_BENCH").ok().map(|spec| {
            let mut parts = spec.strip_prefix("scroll:").unwrap_or_default().split(':');
            let pixels = parts.next().and_then(|p| p.parse().ok());
            let frames = parts.next().and_then(|f| f.parse().ok());
            if pixels.is_none() || frames.is_none() {
                eprintln!("rouge: ROUGE_BENCH must look like `scroll:<px per frame>:<frames>`");
            }
            Bench {
                pixels: pixels.unwrap_or_default(),
                remaining: frames.unwrap_or_default(),
                direction: 1.0,
            }
        });
        if bench.is_none() && var("ROUGE_PROFILE").is_err() {
            return None;
        }
        let stats = ui.stats();
        Some(Profile {
            environment,
            bench,
            intervals: Vec::new(),
            last: None,
            counting: true,
            previous: stats,
            first: stats,
        })
    }

    pub(crate) fn wants_frames(&self) -> bool {
        self.bench.as_ref().is_some_and(|bench| bench.remaining > 0)
    }

    pub(crate) fn finished(&self) -> bool {
        self.bench
            .as_ref()
            .is_some_and(|bench| bench.remaining == 0)
    }

    pub(crate) fn before_update(&mut self, ui: &mut Ui, center: Point) {
        let Some(bench) = self.bench.as_mut().filter(|bench| bench.remaining > 0) else {
            return;
        };
        ui.pointer_moved(center);
        let delta = |direction: f32| Point::new(0.0, -bench.pixels * direction);
        self.counting = ui.scroll(delta(bench.direction)) || {
            bench.direction = -bench.direction;
            ui.scroll(delta(bench.direction))
        };
    }

    pub(crate) fn after_frame(&mut self, ui: &mut Ui) {
        if let Some(bench) = &mut self.bench {
            bench.remaining -= u32::from(self.counting && bench.remaining > 0);
        }
        let now = Instant::now();
        if let Some(last) = self.last.replace(now)
            && self.counting
        {
            self.intervals.push(now - last);
        }
        let stats = ui.stats();
        let frame = stats - self.previous;
        trace!(target: "rouge::frame", ?frame);
        self.previous = stats;
    }

    pub(crate) fn summary(&self) -> String {
        let mut intervals = self.intervals.clone();
        intervals.sort();
        let ms = |p: f64| {
            let index = ((p * intervals.len() as f64) as usize).min(intervals.len().max(1) - 1);
            intervals
                .get(index)
                .map_or(0.0, |interval| interval.as_secs_f64() * 1000.0)
        };
        let over = intervals
            .iter()
            .filter(|interval| **interval > BUDGET)
            .count();
        format!(
            "rouge profile: {} frames, interval ms p50 {:.2} p90 {:.2} p99 {:.2} max {:.2}, {over} over {:.1} ms\n{}\n{:#?}",
            intervals.len(),
            ms(0.5),
            ms(0.9),
            ms(0.99),
            ms(1.0),
            BUDGET.as_secs_f64() * 1000.0,
            self.environment,
            self.previous - self.first,
        )
    }
}
