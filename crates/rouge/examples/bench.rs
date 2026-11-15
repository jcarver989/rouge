use rouge::core::testing::{TestError, TestUi, TestUiBuilder};
use rouge::core::{FrameStats, Wake};
use rouge::prelude::*;
use rouge::render::RenderStats;
use rouge::widgets::button;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::env::args;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::Subscriber;
use tracing::span::Id;
use tracing::subscriber::set_default;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::{LookupSpan, Registry};

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

struct Counting;

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

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
        Some("baseline") => baseline(),
        Some("layout") => layout_bench(arg(2, 20), arg(3, 400)),
        _ => {
            println!(
                "usage: bench baseline | layout [builds] [frames] | scroll [rows] [speed] | glass [panels] [blur] | layers [count]"
            );
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
        while ui.next_frame() == Wake::Now {
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

thread_local! {
    static LAYOUT: Cell<(Option<Instant>, Duration)> = const { Cell::new((None, Duration::ZERO)) };
}

struct LayoutTimer;

impl<T: Subscriber + for<'a> LookupSpan<'a>> Layer<T> for LayoutTimer {
    fn on_enter(&self, id: &Id, cx: Context<'_, T>) {
        if cx.metadata(id).is_some_and(|meta| meta.name() == "layout") {
            LAYOUT.with(|cell| cell.set((Some(Instant::now()), cell.get().1)));
        }
    }

    fn on_exit(&self, id: &Id, cx: Context<'_, T>) {
        if cx.metadata(id).is_some_and(|meta| meta.name() == "layout") {
            LAYOUT.with(|cell| {
                let (start, total) = cell.get();
                cell.set((
                    None,
                    total + start.map_or(Duration::ZERO, |start| start.elapsed()),
                ));
            });
        }
    }
}

fn layout_time() -> Duration {
    LAYOUT.with(|cell| cell.replace((None, Duration::ZERO)).1)
}

fn layout_bench(builds: usize, frames: usize) -> Result<(), TestError> {
    let _subscriber = set_default(Registry::default().with(LayoutTimer));
    for (name, nested) in [("flat", false), ("nested", true)] {
        let mut build = Vec::with_capacity(builds);
        for _ in 0..builds {
            let wide = signal(false);
            layout_time();
            let mut ui = stage(move || cells(wide, nested));
            ui.advance(FRAME);
            build.push(layout_time());
        }
        let wide = signal(false);
        let mut ui = stage(move || cells(wide, nested));
        ui.advance(FRAME);
        ui.advance(FRAME);
        let elements = ui.stats().elements;
        layout_time();
        let before = ui.stats();
        let mut relayout = Vec::with_capacity(frames);
        for frame in 0..frames {
            wide.set(frame % 2 == 0);
            ui.advance(FRAME);
            relayout.push(layout_time());
        }
        let layouts = (ui.stats() - before).layouts;
        println!("{name}: {elements} elements, {layouts} layouts over {frames} frames");
        println!("  layout from scratch:            {}", summary(&mut build));
        println!(
            "  relayout after one width change: {}",
            summary(&mut relayout)
        );
    }
    Ok(())
}

fn summary(times: &mut [Duration]) -> String {
    times.sort();
    let total: Duration = times.iter().sum();
    let count = u32::try_from(times.len().max(1)).unwrap_or(u32::MAX);
    format!(
        "median {:?}, mean {:?}, min {:?}",
        times.get(times.len() / 2).copied().unwrap_or_default(),
        total / count,
        times.first().copied().unwrap_or_default(),
    )
}

fn cells(wide: Signal<bool>, nested: bool) -> Div {
    let cell = move |index: usize| {
        let width = move || if wide.get() { 20.0 } else { 14.0 };
        let cell = div().h(12.0).child(text(format!("{index}")).text_size(8.0));
        if index == 1250 {
            cell.w(width)
        } else {
            cell.w(14.0)
        }
    };
    let root = div().size_full().flex().flex_wrap();
    if nested {
        root.col().children((0..50).map(move |row| {
            div()
                .flex()
                .children((0..50).map(move |column| cell(row * 50 + column)))
        }))
    } else {
        root.children((0..2500).map(cell))
    }
}

fn baseline() -> Result<(), TestError> {
    let reports = [
        static_change()?,
        batched_props()?,
        theme_switch()?,
        enabled_toggle()?,
        idle_editor()?,
        compositor_animation()?,
        scroll_fling()?,
        selection_change()?,
    ];
    println!(
        "| scenario | frames | patches | effects | memos | style resolutions | layouts | layer paints | elements created | group passes | allocations/frame | CPU/frame |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    for report in reports {
        report.print();
    }
    Ok(())
}

struct Report {
    name: &'static str,
    frames: u64,
    work: FrameStats,
    group_passes: u64,
    allocations: u64,
    cpu: Duration,
}

impl Report {
    fn print(&self) {
        let frames = self.frames.max(1);
        let work = &self.work;
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:?} |",
            self.name,
            self.frames,
            work.patches,
            work.reactive.effects_run,
            work.reactive.memos_run,
            work.style_resolutions,
            work.layouts,
            work.layer_paints,
            work.elements_created,
            self.group_passes,
            self.allocations / frames,
            self.cpu / u32::try_from(frames).unwrap_or(u32::MAX),
        );
    }
}

fn measure(
    name: &'static str,
    ui: &mut TestUi,
    run: impl FnOnce(&mut TestUi) -> Result<(u64, u64), TestError>,
) -> Result<Report, TestError> {
    step(ui)?;
    step(ui)?;
    let before = ui.stats();
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let start = Instant::now();
    let (frames, group_passes) = run(ui)?;
    let cpu = start.elapsed();
    eprintln!("{name}: {cpu:?}");
    Ok(Report {
        name,
        frames,
        work: ui.stats() - before,
        group_passes,
        allocations: ALLOCATIONS.load(Ordering::Relaxed) - allocations,
        cpu,
    })
}

fn frame(ui: &mut TestUi) -> Result<u64, TestError> {
    Ok(u64::from(step(ui)?.group_passes))
}

fn static_change() -> Result<Report, TestError> {
    let color = signal(Color::rgb(0x313244));
    let mut ui = stage(move || {
        div()
            .size_full()
            .flex()
            .flex_wrap()
            .children((0..2500).map(move |index| {
                let cell = div()
                    .w(14.0)
                    .h(12.0)
                    .child(text(format!("{index}")).text_size(8.0));
                if index == 1250 {
                    cell.bg(move || color.get())
                } else {
                    cell.bg(0x1e1e2e)
                }
            }))
    });
    measure("one change in 5,000 static elements", &mut ui, |ui| {
        color.set(Color::rgb(0xf38ba8));
        Ok((1, frame(ui)?))
    })
}

fn batched_props() -> Result<Report, TestError> {
    let on = signal(false);
    let pick = move |a: f32, b: f32| move || if on.get() { b } else { a };
    let mut ui = stage(move || {
        div().size_full().child(
            div()
                .w(pick(100.0, 120.0))
                .h(pick(40.0, 48.0))
                .p(pick(4.0, 6.0))
                .px(pick(8.0, 10.0))
                .rounded(pick(2.0, 6.0))
                .border(pick(1.0, 2.0))
                .opacity(pick(1.0, 0.9))
                .text_size(pick(12.0, 14.0))
                .bg(move || {
                    if on.get() {
                        Color::rgb(0xf38ba8)
                    } else {
                        Color::rgb(0x1e1e2e)
                    }
                })
                .border_color(move || if on.get() { Color::WHITE } else { Color::BLACK })
                .child(text("ten props")),
        )
    });
    measure("ten props on one element, one batch", &mut ui, |ui| {
        on.set(true);
        Ok((1, frame(ui)?))
    })
}

fn theme_switch() -> Result<Report, TestError> {
    let current = Rc::new(Cell::new(None));
    let captured = current.clone();
    let mut ui = stage(move || {
        captured.set(Some(Theme::current()));
        div().size_full().flex().flex_wrap().children(
            (0..2000).map(|index| view! { <button test_id={format!("b{index}")} label="Save"/> }),
        )
    });
    measure("theme switch across 2,000 buttons", &mut ui, |ui| {
        if let Some(theme) = current.get() {
            theme.set(UiPalette::dark());
        }
        Ok((1, frame(ui)?))
    })
}

fn enabled_toggle() -> Result<Report, TestError> {
    let disabled = signal(false);
    let mut ui = stage(move || {
        div()
            .size_full()
            .child(button("Save").test_id("save").disabled(disabled))
    });
    measure("toggle a button's enabled state 100 times", &mut ui, |ui| {
        let mut passes = 0;
        for _ in 0..100 {
            disabled.toggle();
            passes += frame(ui)?;
        }
        Ok((100, passes))
    })
}

fn idle_editor() -> Result<Report, TestError> {
    let mut ui = stage(|| {
        div()
            .size_full()
            .child(editable(signal(String::new())).w(200.0).autofocus())
    });
    measure("focused idle editor for 10 s", &mut ui, |ui| {
        ui.press("a");
        let end = ui.time() + 10.5;
        let (mut frames, mut passes) = (0, 0);
        loop {
            match ui.next_frame() {
                Wake::Now => ui.advance(FRAME),
                Wake::At(at) if at <= end => ui.advance(at - ui.time()),
                _ => break,
            };
            passes += u64::from(ui.draw()?.group_passes);
            frames += 1;
        }
        Ok((frames, passes))
    })
}

fn compositor_animation() -> Result<Report, TestError> {
    let on = signal(false);
    let mut ui = stage(move || {
        div().size_full().children((0..100).map(move |index| {
            div()
                .absolute()
                .left((index % 10) as f32 * 40.0)
                .top((index / 10) as f32 * 40.0)
                .size(30.0)
                .bg(Color::hsl(index as f32 * 3.6, 0.7, 0.6))
                .translate_x(move || if on.get() { 300.0 } else { 0.0 })
                .opacity(move || if on.get() { 0.5 } else { 1.0 })
                .transition(Tween::new(2.0))
        }))
    });
    measure("compositor-only animation, 120 frames", &mut ui, |ui| {
        on.set(true);
        let mut passes = 0;
        for _ in 0..120 {
            passes += frame(ui)?;
        }
        Ok((120, passes))
    })
}

fn scroll_fling() -> Result<Report, TestError> {
    let mut ui = stage(|| {
        let row = |index: usize| striped(Color::rgb(0x1e1e2e)).child(text(format!("Row {index}")));
        virtual_list(|| 10_000, ROW_HEIGHT, row).size_full()
    });
    ui.pointer_moved(Point::new(400.0, 300.0));
    measure("scroll fling over 10k rows", &mut ui, |ui| {
        for _ in 0..5 {
            ui.scroll_drag(Point::new(0.0, -60.0));
            ui.advance(FRAME);
        }
        ui.scroll_release();
        let (mut frames, mut passes) = (0, 0);
        while ui.next_frame() == Wake::Now && frames < 1200 {
            passes += frame(ui)?;
            frames += 1;
        }
        Ok((frames, passes))
    })
}

fn selection_change() -> Result<Report, TestError> {
    let selected = signal(Some(0usize));
    let mut ui = stage(move || {
        let selection = selector(move || selected.get());
        div()
            .size_full()
            .flex()
            .col()
            .children((0..1000).map(move |index| {
                let on = selection.is(Some(index));
                div().h(4.0).w_full().bg(move || {
                    if on.get() {
                        Color::rgb(0x89b4fa)
                    } else {
                        Color::rgb(0x1e1e2e)
                    }
                })
            }))
    });
    measure("selection change in 1,000 rows", &mut ui, |ui| {
        selected.set(Some(500));
        Ok((1, frame(ui)?))
    })
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
