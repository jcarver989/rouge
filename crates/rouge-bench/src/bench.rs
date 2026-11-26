use crate::error::BenchError;
use crate::heap::total;
use crate::scenarios::Case;
use rouge::core::Wake;
use rouge::core::testing::{TestError, TestUi, TestUiBuilder};
use rouge::prelude::*;
use rouge::render::RenderStats;
use rouge::widgets::bundled_fonts;
use std::collections::BTreeMap;
use std::time::Instant;

pub(crate) const SIZE: Size = Size::new(800.0, 600.0);
pub type Metrics = BTreeMap<String, f64>;
const FRAME: f64 = 1.0 / 120.0;
const WARMUP: u64 = 10;

#[derive(Default)]
pub(crate) struct Bench {
    size: usize,
    ui: Option<TestUi>,
    work: BTreeMap<&'static str, u64>,
    cpu: Vec<f64>,
    wait: Vec<f64>,
    build_ms: f64,
}

impl Bench {
    pub(crate) fn size(&self) -> usize {
        self.size
    }

    pub(crate) fn mount<T: View, U>(
        &mut self,
        root: impl FnOnce() -> (T, U),
    ) -> Result<U, BenchError> {
        let start = Instant::now();
        let (mut ui, value) = TestUiBuilder::default()
            .size(SIZE.width, SIZE.height)
            .scale(2.0)
            .clear(Color::BLACK)
            .fonts(bundled_fonts())
            .build_with(root);
        ui.advance(FRAME);
        self.build_ms = elapsed(start);
        ui.draw()?;
        ui.wait()?;
        self.ui = Some(ui);
        Ok(value)
    }

    fn ui(&mut self) -> Result<&mut TestUi, BenchError> {
        self.ui.as_mut().ok_or(BenchError::NotMounted)
    }

    pub(crate) fn is_moving(&mut self) -> Result<bool, BenchError> {
        let ui = self.ui()?;
        Ok(ui.next_frame() == Wake::Now || ui.is_animating())
    }

    pub(crate) fn setup<T>(&mut self, act: impl FnOnce(&mut TestUi) -> T) -> Result<T, BenchError> {
        let ui = self.ui()?;
        let (value, _) = draw(ui, act)?;
        ui.wait()?;
        Ok(value)
    }

    pub(crate) fn measure<T>(
        &mut self,
        act: impl FnOnce(&mut TestUi) -> T,
    ) -> Result<T, BenchError> {
        let ui = self.ui()?;
        let (stats, (allocs, bytes)) = (ui.stats(), total());
        let start = Instant::now();
        let (value, render) = measured_frame(ui, act)?;
        let cpu = elapsed(start);
        let heap = total();
        let submitted = Instant::now();
        ui.wait()?;
        let wait = elapsed(submitted);
        let work = ui.stats() - stats;
        for (name, n) in [
            ("frame.layouts", work.layouts),
            ("frame.layer_paints", work.layer_paints),
            ("frame.patches", work.patches),
            ("frame.elements_created", work.elements_created),
            ("frame.elements_removed", work.elements_removed),
            ("frame.text.shapes", work.text.shapes),
            ("frame.text.breaks", work.text.breaks),
            ("render.draw_calls", u64::from(render.draw_calls)),
            (
                "render.instance_bytes_uploaded",
                render.instance_bytes_uploaded,
            ),
            ("heap.allocations", heap.0 - allocs),
            ("heap.bytes", heap.1 - bytes),
        ] {
            *self.work.entry(name).or_default() += n;
        }
        self.cpu.push(cpu);
        self.wait.push(wait);
        Ok(value)
    }

    pub(crate) fn warm_up(
        &mut self,
        mut act: impl FnMut(&mut TestUi, u64),
    ) -> Result<(), BenchError> {
        for frame in 0..WARMUP {
            self.setup(|ui| act(ui, frame))?;
        }
        Ok(())
    }

    pub(crate) fn frames(
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
}

pub fn play(case: &Case) -> Result<Metrics, BenchError> {
    let _runtime = Runtime::new().enter();
    let mut bench = Bench {
        size: case.size,
        ..Bench::default()
    };
    case.run(&mut bench)?;
    let frames = bench.cpu.len();
    if frames == 0 {
        return Err(BenchError::Report(format!("{case} measured no frames")));
    }
    let ui = bench.ui()?;
    let elements = ui.stats().elements;
    let memory = ui.memory()?.total();
    let mut metrics: Metrics = bench
        .work
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v as f64 / frames as f64))
        .collect();
    bench.cpu.sort_unstable_by(f64::total_cmp);
    bench.wait.sort_unstable_by(f64::total_cmp);
    metrics.extend(
        [
            ("frames", frames as f64),
            ("ms.cpu", bench.cpu[frames / 2]),
            ("ms.cpu.p99", bench.cpu[frames * 99 / 100]),
            ("ms.gpu_wait", bench.wait[frames / 2]),
            ("ms.build", bench.build_ms),
            ("frame.elements", elements as f64),
            ("memory.bytes", memory as f64),
        ]
        .map(|(k, v)| (k.to_owned(), v)),
    );
    Ok(metrics)
}

fn elapsed(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

#[inline(never)]
fn measured_frame<T>(
    ui: &mut TestUi,
    act: impl FnOnce(&mut TestUi) -> T,
) -> Result<(T, RenderStats), TestError> {
    draw(ui, act)
}

fn draw<T>(
    ui: &mut TestUi,
    act: impl FnOnce(&mut TestUi) -> T,
) -> Result<(T, RenderStats), TestError> {
    let value = act(ui);
    ui.advance(FRAME);
    Ok((value, ui.draw()?))
}
