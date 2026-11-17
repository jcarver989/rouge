use rouge::core::testing::TestUi;
use rouge::core::{AnyView, KeyEvent, View, Wake};
use rouge::markdown::{MarkdownDocument, MarkdownStream};
use rouge::prelude::*;
use rouge::widgets::{button, markdown};
use std::fmt::Write;

pub(crate) const SIZE: Size = Size::new(800.0, 600.0);
const CENTER: Point = Point::new(SIZE.width / 2.0, SIZE.height / 2.0);
const WARMUP: u64 = 10;
const ROW_HEIGHT: f32 = 24.0;
const GUTTER: f32 = 2.0;
const DRAGS: u64 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Warmup,
    Measure,
    Done,
}

pub(crate) type Script = Box<dyn FnMut(&mut TestUi, u64) -> Step>;
pub(crate) type Staged = (AnyView, Script);
pub(crate) type Scenario = (&'static str, fn() -> Staged);

pub(crate) const SCENARIOS: &[Scenario] = &[
    ("static_change", static_change),
    ("theme_switch", theme_switch),
    ("enabled_toggle", enabled_toggle),
    ("selection_change", selection_change),
    ("idle_editor", idle_editor),
    ("editor_typing", editor_typing),
    ("layers_animation", layers_animation),
    ("glass_scroll", || glass(1)),
    ("glass_panels", || glass(4)),
    ("scroll_list", scroll_list),
    ("scroll_fling", scroll_fling),
    ("layout_flat", || relayout(false)),
    ("layout_nested", || relayout(true)),
    ("resize", resize),
    ("mount_1k", || mount(500)),
    ("mount_10k", || mount(5000)),
    ("mount_100k", || mount(50_000)),
    ("markdown_stream", markdown_stream),
    ("markdown_document", markdown_document),
    ("gallery", gallery),
];

fn static_change() -> Staged {
    flip(200, |hot| {
        let color = move || Color::rgb(if hot.get() { 0xf38ba8 } else { 0x313244 });
        let cells = (0..2500).map(move |index| {
            if index == 1250 {
                cell(index).bg(color)
            } else {
                cell(index).bg(0x1e1e2e)
            }
        });
        div().size_full().flex().flex_wrap().children(cells)
    })
}

fn theme_switch() -> Staged {
    let theme = Theme::current();
    let buttons =
        (0..2000).map(|index| view! { <button test_id={format!("b{index}")} label="Save"/> });
    let view = div().size_full().flex().flex_wrap().children(buttons);
    staged(view, move |_, frame| {
        if frame.is_multiple_of(2) {
            theme.set(UiPalette::dark());
        } else {
            theme.set(UiPalette::light());
        }
        phase(frame, 20)
    })
}

fn enabled_toggle() -> Staged {
    flip(100, |disabled| {
        div()
            .size_full()
            .child(button("Save").test_id("save").disabled(disabled))
    })
}

fn selection_change() -> Staged {
    let selected = signal(Some(0_usize));
    let selection = selector(move || selected.get());
    let rows = (0..1000).map(move |index| {
        let on = selection.is(Some(index));
        let color = move || Color::rgb(if on.get() { 0x89b4fa } else { 0x1e1e2e });
        div().h(4.0).w_full().bg(color)
    });
    let view = div().size_full().flex().col().children(rows);
    staged(view, move |_, frame| {
        selected.set(Some((frame * 37 % 1000) as usize));
        phase(frame, 200)
    })
}

fn idle_editor() -> Staged {
    let view = div()
        .size_full()
        .child(editable(signal(String::new())).w(200.0).autofocus());
    staged(view, |ui, frame| {
        if frame == WARMUP {
            ui.key_down(KeyEvent::typed("a"));
        }
        phase(frame, 1260)
    })
}

fn editor_typing() -> Staged {
    let mut source = String::new();
    for line in 0..2000 {
        let _ = writeln!(
            source,
            "line {line}: the quick brown fox jumps over the lazy dog"
        );
    }
    let view = div()
        .size_full()
        .child(editable(signal(source)).multiline().size_full().autofocus());
    staged(view, |ui, frame| {
        let step = phase(frame, 200);
        if step != Step::Done {
            ui.key_down(KeyEvent::typed("x"));
        }
        step
    })
}

fn layers_animation() -> Staged {
    let on = signal(false);
    let layers = (0..1000).map(move |index| {
        div()
            .absolute()
            .left((index % 40) as f32 * 20.0)
            .top((index / 40) as f32 * 20.0)
            .size(16.0)
            .rounded(Corners::all(4.0))
            .bg(Color::hsl(index as f32 * 3.0, 0.7, 0.6))
            .translate_x(move || if on.get() { 12.0 } else { 0.0 })
            .rotate(move || if on.get() { 90.0 } else { 0.0 })
            .scale(move || if on.get() { 1.3 } else { 1.0 })
            .transition(Spring::bouncy())
    });
    let view = div().size_full().bg(0x1e1e2e).children(layers);
    let mut toggles = 0;
    staged(view, move |ui, frame| {
        if frame < WARMUP {
            return Step::Warmup;
        }
        if ui.next_frame() != Wake::Now && !ui.is_animating() {
            if toggles == 6 {
                return Step::Done;
            }
            toggles += 1;
            on.set(toggles % 2 == 1);
        }
        Step::Measure
    })
}

fn glass(panels: usize) -> Staged {
    let row = |index: usize| {
        striped(Color::hsl(index as f32 * 11.0, 0.6, 0.45))
            .child(text(format!("Row {index} behind the glass")).color(Color::WHITE))
    };
    let panel = move |index: usize| {
        div()
            .absolute()
            .left(0.0)
            .right(GUTTER)
            .top(pct(index as f32 * 100.0 / panels as f32))
            .h(pct(100.0 / panels as f32))
            .backdrop(Glass::new().blur(24.0))
    };
    let view = div()
        .size_full()
        .relative()
        .child(virtual_list(|| 100_000, ROW_HEIGHT, row).size_full())
        .children((0..panels).map(panel));
    scrolled(view, Point::new(SIZE.width - GUTTER / 2.0, CENTER.y), 300)
}

fn scroll_list() -> Staged {
    let row = |index: usize| {
        let shade = if index.is_multiple_of(2) {
            0x1e1e2e
        } else {
            0x313244
        };
        let label = text(format!("Row {index} of a long virtual list")).color(0xcdd6f4);
        striped(Color::rgb(shade)).child(label)
    };
    let view = virtual_list(|| 100_000, ROW_HEIGHT, row).size_full();
    scrolled(view, CENTER, 1000)
}

fn scroll_fling() -> Staged {
    let row = |index: usize| striped(Color::rgb(0x1e1e2e)).child(text(format!("Row {index}")));
    let view = virtual_list(|| 10_000, ROW_HEIGHT, row).size_full();
    staged(view, |ui, frame| {
        if frame < DRAGS {
            ui.pointer_moved(CENTER);
            ui.scroll_drag(Point::new(0.0, -60.0));
            return Step::Warmup;
        }
        if frame == DRAGS {
            ui.scroll_release();
            return Step::Measure;
        }
        let moving = ui.next_frame() == Wake::Now || ui.is_animating();
        if moving && frame < 1200 {
            Step::Measure
        } else {
            Step::Done
        }
    })
}

fn relayout(nested: bool) -> Staged {
    flip(200, |wide| {
        let sized = move |index: usize| {
            if index == 1250 {
                cell(index).w(move || if wide.get() { 20.0 } else { 14.0 })
            } else {
                cell(index)
            }
        };
        let root = div().size_full().flex().flex_wrap();
        if nested {
            root.col().children((0..50).map(move |row| {
                div()
                    .flex()
                    .children((0..50).map(move |column| sized(row * 50 + column)))
            }))
        } else {
            root.children((0..2500).map(sized))
        }
    })
}

fn resize() -> Staged {
    staged(cells(2500), |ui, frame| {
        let scale = if frame.is_multiple_of(2) { 0.8 } else { 1.0 };
        ui.set_size(Size::new(SIZE.width * scale, SIZE.height * scale));
        phase(frame, 100)
    })
}

fn mount(count: usize) -> Staged {
    staged(cells(count), |_, frame| phase(frame, 10))
}

fn markdown_stream() -> Staged {
    let lines: Vec<String> = (document().lines().map(|line| format!("{line}\n"))).collect();
    let stream = signal(MarkdownStream::new());
    let view = div().size_full().p(16.0).child(markdown(stream));
    staged(view, move |_, frame| {
        if let Some(line) = lines.get(frame as usize) {
            stream.update(|stream| {
                stream.push(line);
            });
        }
        phase(frame, 300)
    })
}

fn markdown_document() -> Staged {
    let parsed = MarkdownDocument::parse(document());
    let view = div().size_full().p(16.0).child(markdown(parsed));
    staged(view, |_, frame| phase(frame, 10))
}

fn gallery() -> Staged {
    let name = signal(String::from("Rouge"));
    let rows = (0..24).map(move |row| {
        div()
            .flex()
            .items_center()
            .gap(8.0)
            .child(text(format!("Setting {row}")).w(120.0))
            .children((0..4).map(move |column| {
                button(format!("Action {column}")).test_id(format!("action-{row}-{column}"))
            }))
    });
    let view = div()
        .size_full()
        .flex()
        .col()
        .gap(4.0)
        .p(16.0)
        .child(text(move || format!("Hello, {}", name.get())).text_size(20.0))
        .child(editable(name).w(240.0))
        .children(rows);
    staged(view, |ui, frame| {
        let sweep = (frame % 120) as f32 / 120.0;
        ui.pointer_moved(Point::new(SIZE.width * sweep, SIZE.height * (1.0 - sweep)));
        phase(frame, 600)
    })
}

fn staged(view: impl View, script: impl FnMut(&mut TestUi, u64) -> Step + 'static) -> Staged {
    (view.into_any(), Box::new(script))
}

fn phase(frame: u64, measured: u64) -> Step {
    if frame < WARMUP {
        Step::Warmup
    } else if frame < WARMUP + measured {
        Step::Measure
    } else {
        Step::Done
    }
}

fn flip<T: View>(frames: u64, view: impl FnOnce(Signal<bool>) -> T) -> Staged {
    let on = signal(false);
    staged(view(on), move |_, frame| {
        on.set(frame.is_multiple_of(2));
        phase(frame, frames)
    })
}

fn scrolled(view: impl View, pointer: Point, frames: u64) -> Staged {
    staged(view, move |ui, frame| {
        ui.pointer_moved(pointer);
        ui.scroll(Point::new(0.0, -40.0));
        phase(frame, frames)
    })
}

fn striped(color: Color) -> Div {
    div().size_full().flex().items_center().px(12.0).bg(color)
}

fn cell(index: usize) -> Div {
    div()
        .w(14.0)
        .h(12.0)
        .child(text(format!("{index}")).text_size(8.0))
}

fn cells(count: usize) -> Div {
    div()
        .size_full()
        .flex()
        .flex_wrap()
        .children((0..count).map(cell))
}

fn document() -> String {
    let mut source = String::new();
    for section in 0..30 {
        let _ = write!(
            source,
            "## Section {section}\n\n\
             Rouge renders **bold**, *italic* and `inline code`, and links such as \
             [the docs](https://docs.rs/rouge) or https://example.com/{section}.\n\n\
             - the first point of section {section}\n\
             - a second point with `code`\n\
             - a third point that runs on long enough to wrap onto another line of text\n\n\
             ```rust\n\
             fn section_{section}(values: &[u32]) -> u32 {{\n\
             \x20   let total: u32 = values.iter().sum();\n\
             \x20   println!(\"section {section}: {{total}}\");\n\
             \x20   total * {section}\n\
             }}\n\
             ```\n\n\
             > A quoted remark closes section {section}.\n\n"
        );
    }
    source
}
