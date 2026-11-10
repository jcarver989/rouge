use rouge::core::testing::{TestError, TestUi, TestUiBuilder};
use rouge::prelude::*;
use rouge::render::RenderStats;
use std::env::args;
use std::str::FromStr;
use std::time::{Duration, Instant};

const ROW_HEIGHT: f32 = 24.0;
const GUTTER: f32 = 2.0;
const WARMUP: u32 = 10;
const FRAME: f64 = 1.0 / 120.0;

fn main() -> Result<(), TestError> {
    let _entered = Runtime::new().enter();
    match args().nth(1).as_deref() {
        Some("scroll") => scroll_bench(arg(2, 100_000), arg(3, 40.0)),
        Some("glass") => glass_bench(arg(2, 1), arg(3, 24.0)),
        Some("layers") => layers_bench(arg(2, 1000)),
        _ => {
            println!("usage: bench scroll [rows] [speed] | glass [panels] [blur] | layers [count]");
            Ok(())
        }
    }
}

fn scroll_bench(rows: usize, speed: f32) -> Result<(), TestError> {
    const FRAMES: u32 = 2000;
    let mut ui = stage(move || {
        let row = |index: usize| {
            let shade = if index.is_multiple_of(2) {
                0x1e1e2e
            } else {
                0x313244
            };
            let label = text(format!("Row {index} of a long virtual list")).color(0xcdd6f4);
            striped(Color::rgb(shade)).child(label)
        };
        virtual_list(move || rows, ROW_HEIGHT, row).size_full()
    });
    ui.advance(FRAME);
    ui.pointer_moved(Point::new(400.0, 300.0));

    let before = ui.stats();
    let (mut cpu, mut uploaded) = (Timer::default(), 0);
    for _ in 0..FRAMES {
        let start = Instant::now();
        ui.scroll(Point::new(0.0, -speed));
        uploaded += step(&mut ui)?.instance_bytes_uploaded;
        cpu.record(start.elapsed());
    }
    let after = ui.stats();

    println!("{rows} rows, {speed}px per frame, {FRAMES} frames");
    cpu.report("CPU per frame (scroll + update + encode + submit)");
    println!("layout passes:       {}", after.layouts - before.layouts);
    println!(
        "layers repainted:    {}",
        after.layer_paints - before.layer_paints
    );
    println!(
        "instance bytes uploaded per frame: {}",
        uploaded / u64::from(FRAMES)
    );
    println!("elements mounted:    {}", after.elements);
    Ok(())
}

fn glass_bench(panels: usize, blur: f32) -> Result<(), TestError> {
    const FRAMES: u32 = 600;
    let mut ui = stage(move || {
        let row = |index: usize| {
            striped(Color::hsl(index as f32 * 11.0, 0.6, 0.45))
                .child(text(format!("Row {index} behind the glass")).color(Color::WHITE))
        };
        let glass = move |index: usize| {
            let step = 600.0 / panels as f32;
            div()
                .absolute()
                .left(0.0)
                .right(GUTTER)
                .top(index as f32 * step)
                .h(step)
                .backdrop(Glass::new().blur(blur))
        };
        div()
            .size_full()
            .relative()
            .child(virtual_list(|| 100_000, ROW_HEIGHT, row).size_full())
            .children((0..panels).map(glass))
    });
    ui.advance(FRAME);
    ui.pointer_moved(Point::new(800.0 - GUTTER / 2.0, 300.0));

    let (mut cpu, mut wall) = (Timer::default(), Timer::default());
    let (mut breaks, mut blurs) = (0, 0);
    for frame in 0..FRAMES + WARMUP {
        let start = Instant::now();
        ui.scroll(Point::new(0.0, -40.0));
        let stats = step(&mut ui)?;
        let encoded = start.elapsed();
        ui.wait()?;
        if frame >= WARMUP {
            cpu.record(encoded);
            wall.record(start.elapsed());
            breaks += stats.backdrop_passes;
            blurs += stats.backdrop_blurs;
        }
    }

    println!(
        "{panels} glass panel(s) covering 798x600 of 800x600 at 2x, blur {blur}, {FRAMES} frames of scrolling"
    );
    cpu.report("CPU per frame (scroll + update + encode + submit)");
    wall.report("frame incl. GPU (until the GPU finished it)");
    println!(
        "pass breaks per frame:    {:.2}",
        breaks as f32 / FRAMES as f32
    );
    println!(
        "backdrop blurs per frame: {:.2}",
        blurs as f32 / FRAMES as f32
    );
    Ok(())
}

fn layers_bench(layers: usize) -> Result<(), TestError> {
    let on = signal(false);
    let mut ui = stage(move || {
        div()
            .size_full()
            .bg(0x1e1e2e)
            .children((0..layers).map(move |i| {
                let (col, row) = ((i % 40) as f32, (i / 40) as f32);
                div()
                    .absolute()
                    .left(col * 20.0)
                    .top(row * 20.0)
                    .size(16.0)
                    .rounded(4.0)
                    .bg(Color::hsl(i as f32 * 3.0, 0.7, 0.6))
                    .translate_x(move || if on.get() { 12.0 } else { 0.0 })
                    .rotate(move || if on.get() { 90.0 } else { 0.0 })
                    .scale(move || if on.get() { 1.3 } else { 1.0 })
                    .transition(Spring::bouncy())
            }))
    });

    let start = Instant::now();
    let uploaded = step(&mut ui)?.instance_bytes_uploaded;
    println!(
        "{layers} layers; first frame {:?} ({uploaded} instance bytes uploaded)",
        start.elapsed()
    );

    let before = ui.stats();
    let (mut update, mut cpu, mut uploaded) = (Timer::default(), Timer::default(), 0);
    for toggle in 0..6 {
        on.set(toggle % 2 == 0);
        while ui.needs_frame() {
            let start = Instant::now();
            ui.advance(FRAME);
            update.record(start.elapsed());
            uploaded += ui.draw()?.instance_bytes_uploaded;
            cpu.record(start.elapsed());
        }
    }
    let after = ui.stats();

    println!("animated frames:        {}", cpu.frames);
    update.report("update (tick + compose)");
    cpu.report("CPU per frame (update + encode + submit)");
    println!("instance bytes uploaded during animation: {uploaded}");
    println!(
        "layout passes during animation: {}, paint passes: {}",
        after.layouts - before.layouts,
        after.paints - before.paints
    );
    Ok(())
}

fn arg<T: FromStr>(index: usize, default: T) -> T {
    args()
        .nth(index)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn striped(color: Color) -> Div {
    div().size_full().flex().items_center().px(12.0).bg(color)
}

fn stage<T: View>(root: impl FnOnce() -> T) -> TestUi {
    TestUiBuilder::default()
        .size(800.0, 600.0)
        .scale(2.0)
        .clear(Color::BLACK)
        .build(root)
}

fn step(ui: &mut TestUi) -> Result<RenderStats, TestError> {
    ui.advance(FRAME);
    ui.draw()
}

#[derive(Default)]
struct Timer {
    total: Duration,
    worst: Duration,
    frames: u32,
}

impl Timer {
    fn record(&mut self, elapsed: Duration) {
        self.total += elapsed;
        self.worst = self.worst.max(elapsed);
        self.frames += 1;
    }

    fn report(&self, what: &str) {
        let average = self.total / self.frames.max(1);
        println!("avg {what}: {average:?}, worst {:?}", self.worst);
    }
}
