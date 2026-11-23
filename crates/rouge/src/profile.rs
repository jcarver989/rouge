use crate::platform::{log, warn};
use rouge_core::{FrameStats, Ui};
use rouge_render::Point;
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use tracing::level_filters::LevelFilter;
use tracing::subscriber::set_global_default;
use tracing::trace;
#[cfg(not(target_arch = "wasm32"))]
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::Registry;
#[cfg(not(target_arch = "wasm32"))]
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::{Layer, SubscriberExt};
use web_time::Instant;
use wgpu::PresentMode;

const BUDGET: Duration = Duration::from_micros(16_667);

#[derive(Clone, Debug)]
pub(crate) struct ProfileSettings {
    pub(crate) bench: Option<String>,
    pub(crate) trace: Option<String>,
    pub(crate) present_mode: Option<PresentMode>,
    pub(crate) exit_after_frames: Option<u64>,
}

impl ProfileSettings {
    pub(crate) fn read(setting: impl Fn(&str) -> Option<String>) -> Self {
        ProfileSettings {
            bench: setting("rouge_bench"),
            trace: setting("rouge_profile"),
            present_mode: setting("rouge_present_mode").and_then(|mode| present_mode(&mode)),
            exit_after_frames: setting("rouge_exit_after_frames").and_then(|n| n.parse().ok()),
        }
    }
}

fn present_mode(mode: &str) -> Option<PresentMode> {
    match mode.to_ascii_lowercase().as_str() {
        "fifo" => Some(PresentMode::Fifo),
        "fifo-relaxed" => Some(PresentMode::FifoRelaxed),
        "mailbox" => Some(PresentMode::Mailbox),
        "immediate" => Some(PresentMode::Immediate),
        "auto-vsync" => Some(PresentMode::AutoVsync),
        "auto-no-vsync" => Some(PresentMode::AutoNoVsync),
        _ => {
            warn(&format!("unknown present mode `{mode}`"));
            None
        }
    }
}

pub(crate) fn subscribe(layer: impl Layer<Registry> + Send + Sync + 'static) -> bool {
    let subscribed = set_global_default(Registry::default().with(layer)).is_ok();
    if !subscribed {
        warn("profiling is ignored because a tracing subscriber is already set");
    }
    subscribed
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
    pub(crate) fn new(
        stats: FrameStats,
        settings: &ProfileSettings,
        environment: impl FnOnce() -> String,
    ) -> Result<Option<Profile>, String> {
        let bench = match &settings.bench {
            Some(spec) => Some(parse_bench(spec)?),
            None => None,
        };
        if bench.is_none() && settings.trace.is_none() {
            return Ok(None);
        }
        Ok(Some(Profile {
            environment: environment(),
            bench,
            intervals: Vec::new(),
            last: None,
            counting: true,
            previous: stats,
            first: stats,
        }))
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

    fn summary(&self) -> String {
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

impl Drop for Profile {
    fn drop(&mut self) {
        log(&self.summary());
    }
}

fn parse_bench(spec: &str) -> Result<Bench, String> {
    let mut parts = spec.strip_prefix("scroll:").unwrap_or_default().split(':');
    let pixels = parts.next().and_then(|p| p.parse().ok());
    let frames = parts.next().and_then(|f| f.parse().ok());
    match (pixels, frames) {
        (Some(pixels), Some(remaining)) => Ok(Bench {
            pixels,
            remaining,
            direction: 1.0,
        }),
        _ => Err(format!(
            "a bench must look like `scroll:<px per frame>:<frames>`, not `{spec}`"
        )),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn trace(settings: &ProfileSettings) -> Option<FlushGuard> {
    let path = settings.trace.as_deref().filter(|path| !path.is_empty())?;
    let (layer, guard) = ChromeLayerBuilder::new()
        .file(path)
        .include_args(true)
        .build();
    let layer = layer.with_filter(
        Targets::new()
            .with_target("winit", LevelFilter::OFF)
            .with_default(LevelFilter::TRACE),
    );
    subscribe(layer).then(|| {
        eprintln!("rouge profile: tracing to {path}");
        guard
    })
}
