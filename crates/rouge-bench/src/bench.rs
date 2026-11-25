use crate::error::BenchError;
use crate::heap::{HeapUse, total};
use crate::metrics::{Metrics, millis, per_frame, percentile, scrape};
use crate::scenarios::Case;
use rouge::core::testing::{TestError, TestUi, TestUiBuilder};
use rouge::core::{FrameStats, Wake};
use rouge::prelude::*;
use rouge::render::RenderStats;
use rouge::widgets::bundled_fonts;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use tracing::Subscriber;
use tracing::span::Id;
use tracing::subscriber::set_default;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

pub const SIZE: Size = Size::new(800.0, 600.0);
pub const MEASURED: &str = "rouge_bench::bench::measured_frame";

const FRAME: f64 = 1.0 / 120.0;
const WARMUP: u64 = 10;

struct Span {
    name: &'static str,
    time: Duration,
    root: bool,
}

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Observe {
    #[default]
    Counts,
    Times,
    Allocations,
}

struct Frame {
    cpu: Duration,
    gpu: Duration,
    heap: HeapUse,
    stats: FrameStats,
    render: RenderStats,
}

struct Built {
    time: Duration,
    heap: HeapUse,
}

pub struct Bench {
    size: usize,
    observe: Observe,
    ui: Option<TestUi>,
    built: Option<Built>,
    frames: Vec<Frame>,
    incoming: Receiver<Span>,
    spans: BTreeMap<&'static str, Duration>,
    spanned: Duration,
}

impl Bench {
    pub fn size(&self) -> usize {
        self.size
    }

    pub fn mount<T: View, U>(&mut self, root: impl FnOnce() -> (T, U)) -> Result<U, BenchError> {
        let (heap, start) = (total(), Instant::now());
        let (mut ui, value) = TestUiBuilder::default()
            .size(SIZE.width, SIZE.height)
            .scale(2.0)
            .clear(Color::BLACK)
            .fonts(bundled_fonts())
            .build_with(root);
        ui.advance(FRAME);
        self.built = Some(Built {
            time: start.elapsed(),
            heap: total() - heap,
        });
        ui.draw()?;
        ui.wait()?;
        self.ui = Some(ui);
        Ok(value)
    }

    pub fn ui(&mut self) -> Result<&mut TestUi, BenchError> {
        self.ui.as_mut().ok_or(BenchError::NotMounted)
    }

    pub fn is_moving(&mut self) -> Result<bool, BenchError> {
        let ui = self.ui()?;
        Ok(ui.next_frame() == Wake::Now || ui.is_animating())
    }

    pub fn setup<T>(&mut self, act: impl FnOnce(&mut TestUi) -> T) -> Result<T, BenchError> {
        let ui = self.ui()?;
        let value = act(ui);
        ui.advance(FRAME);
        ui.draw()?;
        ui.wait()?;
        Ok(value)
    }

    pub fn measure<T>(&mut self, act: impl FnOnce(&mut TestUi) -> T) -> Result<T, BenchError> {
        let ui = self.ui.as_mut().ok_or(BenchError::NotMounted)?;
        self.incoming.try_iter().for_each(drop);
        let (stats, heap) = (ui.stats(), total());
        arm(self.observe == Observe::Allocations);
        let start = Instant::now();
        let frame = measured_frame(ui, act);
        let cpu = start.elapsed();
        arm(false);
        let (value, render) = frame?;
        let heap = total() - heap;
        let submitted = Instant::now();
        ui.wait()?;
        self.frames.push(Frame {
            cpu,
            gpu: submitted.elapsed(),
            heap,
            stats: ui.stats() - stats,
            render,
        });
        for span in self.incoming.try_iter() {
            *self.spans.entry(span.name).or_default() += span.time;
            self.spanned += if span.root { span.time } else { Duration::ZERO };
        }
        Ok(value)
    }

    pub fn warm_up(&mut self, mut act: impl FnMut(&mut TestUi, u64)) -> Result<(), BenchError> {
        for frame in 0..WARMUP {
            self.setup(|ui| act(ui, frame))?;
        }
        Ok(())
    }

    pub fn frames(
        &mut self,
        count: u64,
        mut act: impl FnMut(&mut TestUi, u64),
    ) -> Result<(), BenchError> {
        self.warm_up(&mut act)?;
        for frame in WARMUP..WARMUP + count {
            self.measure(|ui| act(ui, frame))?;
        }
        Ok(())
    }

    pub fn measured(&self) -> (usize, Duration) {
        (
            self.frames.len(),
            self.frames.iter().map(|frame| frame.cpu).sum(),
        )
    }

    pub fn metrics(&mut self) -> Result<Metrics, BenchError> {
        let count = self.frames.len();
        let mut metrics = Metrics::new();
        let mut heap = HeapUse::default();
        for frame in &self.frames {
            heap += frame.heap;
            scrape(&mut metrics, "frame", &frame.stats);
            scrape(&mut metrics, "render", &frame.render);
        }
        for (key, total) in [
            ("heap.allocations", heap.allocations),
            ("heap.bytes", heap.bytes),
        ] {
            metrics.insert(key.to_owned(), total as f64);
        }
        for (name, time) in &self.spans {
            metrics.insert(format!("ms.span.{name}"), millis(*time));
        }
        if !self.spans.is_empty() {
            let outside = self.measured().1.saturating_sub(self.spanned);
            metrics.insert("ms.outside".to_owned(), millis(outside));
        }
        per_frame(&mut metrics, count);
        let cpu = |p| percentile(self.frames.iter().map(|frame| frame.cpu), p);
        let gpu = percentile(self.frames.iter().map(|frame| frame.gpu), 0.5);
        let times = [
            ("frames", count as f64),
            ("ms.cpu", cpu(0.5)),
            ("ms.cpu.p99", cpu(0.99)),
            ("ms.gpu", gpu),
        ];
        metrics.extend(times.map(|(key, value)| (key.to_owned(), value)));
        if let Some(built) = &self.built {
            let build = [
                ("heap.build.allocations", built.heap.allocations as f64),
                ("heap.build.bytes", built.heap.bytes as f64),
                ("ms.build", millis(built.time)),
            ];
            metrics.extend(build.map(|(key, value)| (key.to_owned(), value)));
        }
        scrape(&mut metrics, "memory", &self.ui()?.memory()?);
        Ok(metrics)
    }
}

pub fn play<T>(
    case: &Case,
    observe: Observe,
    finish: impl FnOnce(&mut Bench) -> Result<T, BenchError>,
) -> Result<T, BenchError> {
    let _entered = Runtime::new().enter();
    let (sender, incoming) = channel();
    let timed = observe == Observe::Times;
    let _subscribed = timed.then(|| set_default(Registry::default().with(Spans(sender))));
    let mut bench = Bench {
        size: case.size().unwrap_or_default(),
        observe,
        ui: None,
        built: None,
        frames: Vec::with_capacity(1 << 12),
        incoming,
        spans: BTreeMap::new(),
        spanned: Duration::ZERO,
    };
    case.run(&mut bench)?;
    finish(&mut bench)
}

#[inline(never)]
fn measured_frame<T>(
    ui: &mut TestUi,
    act: impl FnOnce(&mut TestUi) -> T,
) -> Result<(T, RenderStats), TestError> {
    let value = act(ui);
    ui.advance(FRAME);
    let render = ui.draw()?;
    Ok((value, render))
}

#[cfg(feature = "profile")]
fn arm(on: bool) {
    crate::heap::arm(on);
}

#[cfg(not(feature = "profile"))]
fn arm(_: bool) {}

struct Spans(Sender<Span>);

impl<T: Subscriber + for<'a> LookupSpan<'a>> Layer<T> for Spans {
    fn on_enter(&self, id: &Id, cx: Context<'_, T>) {
        if let Some(span) = cx.span(id) {
            DEPTH.set(DEPTH.get() + 1);
            span.extensions_mut().replace(Instant::now());
        }
    }

    fn on_exit(&self, id: &Id, cx: Context<'_, T>) {
        let Some(span) = cx.span(id) else {
            return;
        };
        if let Some(entered) = span.extensions_mut().remove::<Instant>() {
            DEPTH.set(DEPTH.get().saturating_sub(1));
            let _ = self.0.send(Span {
                name: span.name(),
                time: entered.elapsed(),
                root: DEPTH.get() == 0,
            });
        }
    }
}
