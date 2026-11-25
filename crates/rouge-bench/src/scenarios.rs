mod rows;

use crate::bench::{Bench, SIZE};
use crate::error::BenchError;
use rouge::core::{KeyEvent, View};
use rouge::markdown::{MarkdownDocument, MarkdownStream};
use rouge::prelude::*;
use rouge::widgets::{button, column, data_table, markdown, table_rows};
use rows::Operation;
use std::collections::VecDeque;
use std::fmt::{Display, Formatter, Result as FmtResult, Write};

const CENTER: Point = Point::new(SIZE.width / 2.0, SIZE.height / 2.0);
const ROW_HEIGHT: f32 = 24.0;
const GUTTER: f32 = 2.0;
const DRAGS: u64 = 5;
const TOGGLES: u64 = 6;
const FLING_FRAMES: u64 = 1200;

type Outcome = Result<(), BenchError>;

pub struct Scenario {
    pub name: &'static str,
    minimum: usize,
    pub sizes: &'static [usize],
    run: fn(&mut Bench) -> Outcome,
}

const fn plain(name: &'static str, run: fn(&mut Bench) -> Outcome) -> Scenario {
    sized(name, &[], run)
}

const fn sized(
    name: &'static str,
    sizes: &'static [usize],
    run: fn(&mut Bench) -> Outcome,
) -> Scenario {
    Scenario {
        name,
        minimum: 1,
        sizes,
        run,
    }
}

impl Scenario {
    const fn at_least(mut self, minimum: usize) -> Self {
        self.minimum = minimum;
        self
    }

    fn case(&'static self, size: Option<usize>) -> Result<Case, BenchError> {
        if let Some(size) = size {
            if self.sizes.is_empty() {
                return Err(BenchError::Unsized(self.name.to_owned()));
            }
            if size < self.minimum {
                return Err(BenchError::BelowMinimum {
                    name: self.name,
                    size,
                    minimum: self.minimum,
                });
            }
        }
        Ok(Case {
            scenario: self,
            size: size.or_else(|| self.sizes.first().copied()),
        })
    }
}

pub const SCENARIOS: &[Scenario] = &[
    sized("static_change", &[2500], static_change),
    sized("theme_switch", &[2000], theme_switch),
    plain("enabled_toggle", enabled_toggle),
    sized("selection_change", &[1000], selection_change),
    plain("idle_editor", idle_editor),
    sized("editor_typing", &[2000], |b| editor(b, Edit::Type, false)),
    sized("editor_typing_access", &[2000], |b| {
        editor(b, Edit::Type, true)
    }),
    sized("editor_caret", &[2000], |b| editor(b, Edit::Move, false)),
    sized("editor_caret_access", &[2000], |b| {
        editor(b, Edit::Move, true)
    }),
    sized("layers_animation", &[1000], layers_animation),
    plain("glass_scroll", |b| glass(b, 1)),
    plain("glass_panels", |b| glass(b, 4)),
    sized("scroll_list", &[100_000], scroll_list),
    sized("scroll_fling", &[10_000], scroll_fling),
    sized("keyed_prepend", &[1000, 100_000], keyed_prepend)
        .at_least(2 * (SIZE.height as usize).div_ceil(ROW_HEIGHT as usize)),
    sized("row_identity_prepend", &[100_000], row_identity_prepend),
    sized("data_table_scroll", &[100_000], data_table_scroll),
    sized("rows_create", &[1000, 10_000], |b| {
        rows::run(b, Operation::Create)
    }),
    sized("table_virtual_create", &[10_000], rows::virtual_create),
    sized("rows_replace", &[1000], |b| {
        rows::run(b, Operation::Replace)
    }),
    sized("rows_append", &[1000], |b| rows::run(b, Operation::Append)),
    sized("rows_update_10th", &[1000], |b| {
        rows::run(b, Operation::Update)
    }),
    sized("rows_select", &[1000], |b| rows::run(b, Operation::Select))
        .at_least(Operation::Select.minimum()),
    sized("rows_swap", &[1000], |b| rows::run(b, Operation::Swap))
        .at_least(Operation::Swap.minimum()),
    sized("rows_remove", &[1000], |b| rows::run(b, Operation::Remove))
        .at_least(Operation::Remove.minimum()),
    sized("rows_clear", &[1000], |b| rows::run(b, Operation::Clear)),
    sized("layout_flat", &[2500], |b| relayout(b, false)),
    sized("layout_nested", &[2500], |b| relayout(b, true)),
    sized("resize", &[2500], resize),
    sized("mount", &[1000, 10_000, 100_000], mount),
    plain("markdown_stream", markdown_stream),
    plain("markdown_document", markdown_document),
    plain("gallery", gallery),
];

#[derive(Clone, Copy)]
pub struct Case {
    scenario: &'static Scenario,
    size: Option<usize>,
}

impl Case {
    pub fn all() -> Vec<Case> {
        SCENARIOS.iter().flat_map(Case::defaults).collect()
    }

    pub fn exact(name: &str) -> Result<Case, BenchError> {
        let (name, size) = split(name)?;
        let found = SCENARIOS.iter().find(|scenario| scenario.name == name);
        let scenario = found.ok_or_else(|| BenchError::NoScenario(name.to_owned()))?;
        scenario.case(size)
    }

    pub fn matching(filters: &[String]) -> Result<Vec<Case>, BenchError> {
        if filters.is_empty() {
            return Ok(Case::all());
        }
        let mut cases: Vec<Case> = Vec::new();
        for filter in filters {
            let (part, size) = split(filter)?;
            let named = |scenario: &&Scenario| scenario.name.contains(part);
            let scenarios: Vec<&Scenario> = SCENARIOS.iter().filter(named).collect();
            let found: Vec<Case> = match size {
                Some(size) => scenarios
                    .iter()
                    .filter(|s| !s.sizes.is_empty())
                    .map(|scenario| scenario.case(Some(size)))
                    .collect::<Result<_, _>>()?,
                None => scenarios.iter().flat_map(|s| Case::defaults(s)).collect(),
            };
            if found.is_empty() {
                return Err(if scenarios.is_empty() {
                    BenchError::NoScenario(filter.clone())
                } else {
                    BenchError::Unsized(part.to_owned())
                });
            }
            for case in found {
                if !cases.iter().any(|known| known.is(&case)) {
                    cases.push(case);
                }
            }
        }
        Ok(cases)
    }

    pub fn single(filter: &str) -> Result<Case, BenchError> {
        if let Ok(case) = Case::exact(filter) {
            return Ok(case);
        }
        let cases = Case::matching(&[filter.to_owned()])?;
        let [case] = cases[..] else {
            let names: Vec<String> = cases.iter().map(Case::to_string).collect();
            return Err(BenchError::Ambiguous(cases.len(), names.join(", ")));
        };
        Ok(case)
    }

    pub fn name(&self) -> &'static str {
        self.scenario.name
    }

    pub fn size(&self) -> Option<usize> {
        self.size
    }

    pub fn resized(&self, size: usize) -> Result<Case, BenchError> {
        self.scenario.case(Some(size))
    }

    pub(crate) fn run(&self, bench: &mut Bench) -> Outcome {
        (self.scenario.run)(bench)
    }

    fn is(&self, other: &Case) -> bool {
        self.scenario.name == other.scenario.name && self.size == other.size
    }

    fn defaults(scenario: &'static Scenario) -> Vec<Case> {
        let sizes = scenario.sizes.iter().copied().map(Some);
        let mut cases: Vec<Case> = sizes.map(|size| Case { scenario, size }).collect();
        if cases.is_empty() {
            cases.push(Case {
                scenario,
                size: None,
            });
        }
        cases
    }
}

impl Display for Case {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self.size {
            Some(size) => write!(f, "{}@{size}", self.scenario.name),
            None => f.write_str(self.scenario.name),
        }
    }
}

fn split(filter: &str) -> Result<(&str, Option<usize>), BenchError> {
    let Some((name, size)) = filter.split_once('@') else {
        return Ok((filter, None));
    };
    let parsed = size.replace('_', "").parse().ok().filter(|size| *size > 0);
    let size = parsed.ok_or_else(|| BenchError::BadSize(filter.to_owned()))?;
    Ok((name, Some(size)))
}

fn static_change(bench: &mut Bench) -> Outcome {
    let count = bench.size();
    flip(bench, 200, |hot| {
        let color = move || Color::rgb(if hot.get() { 0xf38ba8 } else { 0x313244 });
        let cells = (0..count).map(move |index| {
            if index == count / 2 {
                cell(index).bg(color)
            } else {
                cell(index).bg(0x1e1e2e)
            }
        });
        div().size_full().flex().flex_wrap().children(cells)
    })
}

fn theme_switch(bench: &mut Bench) -> Outcome {
    let count = bench.size();
    let theme = bench.mount(|| {
        let buttons =
            (0..count).map(|index| view! { <button test_id={format!("b{index}")} label="Save"/> });
        let view = div().size_full().flex().flex_wrap().children(buttons);
        (view, Theme::current())
    })?;
    bench.frames(20, |_, frame| {
        theme.set(UiPalette::for_dark(frame.is_multiple_of(2)));
    })
}

fn enabled_toggle(bench: &mut Bench) -> Outcome {
    flip(bench, 100, |disabled| {
        div()
            .size_full()
            .child(button("Save").test_id("save").disabled(disabled))
    })
}

fn selection_change(bench: &mut Bench) -> Outcome {
    let count = bench.size();
    let selected = bench.mount(|| {
        let selected = signal(Some(0_usize));
        let selection = selector(move || selected.get());
        let rows = (0..count).map(move |index| {
            let on = selection.is(Some(index));
            let color = move || Color::rgb(if on.get() { 0x89b4fa } else { 0x1e1e2e });
            div().h(4.0).w_full().bg(color)
        });
        (div().size_full().flex().col().children(rows), selected)
    })?;
    bench.frames(200, |_, frame| {
        selected.set(Some((frame * 37 % count as u64) as usize));
    })
}

fn idle_editor(bench: &mut Bench) -> Outcome {
    bench.mount(|| {
        let editor = editable(signal(String::new())).w(200.0).autofocus();
        (div().size_full().child(editor), ())
    })?;
    bench.warm_up(|_, _| {})?;
    bench.measure(|ui| {
        ui.key_down(KeyEvent::typed("a"));
    })?;
    for _ in 1..1260 {
        bench.measure(|_| {})?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Edit {
    Type,
    Move,
}

fn editor(bench: &mut Bench, edit: Edit, access: bool) -> Outcome {
    let mut source = String::new();
    for line in 0..bench.size() {
        let _ = writeln!(
            source,
            "line {line}: the quick brown fox jumps over the lazy dog"
        );
    }
    bench.mount(|| {
        let editor = editable(signal(source)).multiline().size_full().autofocus();
        (div().size_full().child(editor), ())
    })?;
    bench.frames(200, |ui, frame| {
        if access {
            ui.access_tree();
        }
        match edit {
            Edit::Type => {
                ui.key_down(KeyEvent::typed("x"));
            }
            Edit::Move => {
                ui.press(if frame.is_multiple_of(2) {
                    "right"
                } else {
                    "left"
                });
            }
        }
    })
}

fn layers_animation(bench: &mut Bench) -> Outcome {
    let count = bench.size();
    let on = bench.mount(|| {
        let on = signal(false);
        let layers = (0..count).map(move |index| {
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
        (div().size_full().bg(0x1e1e2e).children(layers), on)
    })?;
    bench.warm_up(|_, _| {})?;
    for toggle in 0..TOGGLES {
        bench.measure(|_| on.set(toggle.is_multiple_of(2)))?;
        while bench.is_moving()? {
            bench.measure(|_| {})?;
        }
    }
    Ok(())
}

fn glass(bench: &mut Bench, panels: usize) -> Outcome {
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
    bench.mount(|| {
        let view = div()
            .size_full()
            .relative()
            .child(virtual_list(|| 100_000, ROW_HEIGHT, row).size_full())
            .children((0..panels).map(panel));
        (view, ())
    })?;
    scroll_by(bench, Point::new(SIZE.width - GUTTER / 2.0, CENTER.y), 300)
}

fn scroll_list(bench: &mut Bench) -> Outcome {
    let count = bench.size();
    let row = |index: usize| {
        let shade = if index.is_multiple_of(2) {
            0x1e1e2e
        } else {
            0x313244
        };
        let label = text(format!("Row {index} of a long virtual list")).color(0xcdd6f4);
        striped(Color::rgb(shade)).child(label)
    };
    bench.mount(|| {
        let list = virtual_list(move || count, ROW_HEIGHT, row);
        (list.size_full(), ())
    })?;
    scroll_by(bench, CENTER, 1000)
}

fn keyed_prepend(bench: &mut Bench) -> Outcome {
    let count = bench.size();
    let (model, handle) = bench.mount(|| {
        let model = keyed_list(|key: &usize| *key);
        model.extend(0..count);
        let handle = virtual_list_handle();
        let view = virtual_list(model, ROW_HEIGHT, move |index| {
            let key = key_at(model, index).expect("mounted row has a key");
            div().size_full().child(text(format!("Message {key}")))
        });
        (view.handle(handle).size_full(), (model, handle))
    })?;
    bench.setup(|_| handle.scroll_to_row(count / 2, Align::Top))?;
    bench.warm_up(|_, _| {})?;
    for added in 0..100 {
        let top = handle.top_row().get_untracked();
        assert_eq!(key_at(model, top), Some(count / 2));
        bench.measure(|_| model.prepend(count + added))?;
    }
    Ok(())
}

fn key_at(model: KeyedList<usize, usize>, index: usize) -> Option<usize> {
    model
        .order()
        .with_untracked(|order| order.get(index).copied())
}

fn row_identity_prepend(bench: &mut Bench) -> Outcome {
    let count = bench.size();
    let (keys, handle) = bench.mount(|| {
        let keys = signal((0..count).collect::<VecDeque<_>>());
        let handle = virtual_list_handle();
        let view = virtual_list(
            move || keys.with(VecDeque::len),
            ROW_HEIGHT,
            move |index| {
                let key = keys.with_untracked(|keys| keys[index]);
                div().size_full().child(text(format!("Message {key}")))
            },
        )
        .row_identity(move |index| RowKey::new(keys.with(|keys| keys[index]), 0))
        .handle(handle)
        .size_full();
        (view, (keys, handle))
    })?;
    bench.setup(|_| handle.scroll_to_row(count / 2, Align::Top))?;
    bench.warm_up(|_, _| {})?;
    for added in 0..100 {
        bench.measure(|_| keys.update(|keys| keys.push_front(count + added)))?;
    }
    Ok(())
}

fn data_table_scroll(bench: &mut Bench) -> Outcome {
    let count = bench.size();
    bench.mount(|| {
        let rows = table_rows(move || count, |index| index);
        let table = view! {
            <data_table rows=rows
                 column=column("Title", |index: &usize| view! { <text>{format!("Track {index}")}</text> }).flex(1.4)
                 column={column("Artist", |index: &usize| {
                     view! { <text>{format!("Artist {}", index % 97)}</text> }
                 })}
                 column={column("Plays", |index: &usize| {
                     view! { <text>{(index * 7919 % 100_000).to_string()}</text> }
                 })}
                 column={column("Length", |index: &usize| {
                     view! { <text>{format!("{}:{:02}", 2 + index % 5, index % 60)}</text> }
                 })}/>
        };
        (view! { <div size_full flex col>{table}</div> }, ())
    })?;
    scroll_by(bench, CENTER, 1000)
}

fn scroll_fling(bench: &mut Bench) -> Outcome {
    let count = bench.size();
    let row = |index: usize| striped(Color::rgb(0x1e1e2e)).child(text(format!("Row {index}")));
    bench.mount(|| {
        let list = virtual_list(move || count, ROW_HEIGHT, row);
        (list.size_full(), ())
    })?;
    for _ in 0..DRAGS {
        bench.setup(|ui| {
            ui.pointer_moved(CENTER);
            ui.scroll_drag(Point::new(0.0, -60.0));
        })?;
    }
    bench.measure(|ui| {
        ui.scroll_release();
    })?;
    for _ in DRAGS + 1..FLING_FRAMES {
        if !bench.is_moving()? {
            break;
        }
        bench.measure(|_| {})?;
    }
    Ok(())
}

fn relayout(bench: &mut Bench, nested: bool) -> Outcome {
    let count = bench.size();
    let columns = count.isqrt().max(1);
    flip(bench, 200, |wide| {
        let sized = move |index: usize| {
            if index == count / 2 {
                cell(index).w(move || if wide.get() { 20.0 } else { 14.0 })
            } else {
                cell(index)
            }
        };
        let root = div().size_full().flex().flex_wrap();
        if nested {
            root.col()
                .children((0..count).step_by(columns).map(move |start| {
                    div()
                        .flex()
                        .children((start..start.saturating_add(columns).min(count)).map(sized))
                }))
        } else {
            root.children((0..count).map(sized))
        }
    })
}

fn resize(bench: &mut Bench) -> Outcome {
    let count = bench.size();
    bench.mount(|| (cells(count), ()))?;
    bench.frames(100, |ui, frame| {
        let scale = if frame.is_multiple_of(2) { 0.8 } else { 1.0 };
        ui.set_size(Size::new(SIZE.width * scale, SIZE.height * scale));
    })
}

fn mount(bench: &mut Bench) -> Outcome {
    let elements = bench.size();
    bench.mount(|| {
        let grid = cells(elements / 2);
        let grid = if elements.is_multiple_of(2) {
            grid
        } else {
            grid.child(div().w(14.0).h(12.0))
        };
        (grid, ())
    })?;
    bench.frames(10, |_, _| {})
}

fn markdown_stream(bench: &mut Bench) -> Outcome {
    let lines: Vec<String> = (document().lines().map(|line| format!("{line}\n"))).collect();
    let stream = bench.mount(|| {
        let stream = signal(MarkdownStream::new());
        let view = div().size_full().p(16.0).child(markdown(stream));
        (view, stream)
    })?;
    bench.frames(300, |_, frame| {
        if let Some(line) = lines.get(frame as usize) {
            stream.update(|stream| {
                stream.push(line);
            });
        }
    })
}

fn markdown_document(bench: &mut Bench) -> Outcome {
    bench.mount(|| {
        let parsed = MarkdownDocument::parse(document());
        (div().size_full().p(16.0).child(markdown(parsed)), ())
    })?;
    bench.frames(10, |_, _| {})
}

fn gallery(bench: &mut Bench) -> Outcome {
    bench.mount(|| {
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
        (view, ())
    })?;
    bench.frames(600, |ui, frame| {
        let sweep = (frame % 120) as f32 / 120.0;
        ui.pointer_moved(Point::new(SIZE.width * sweep, SIZE.height * (1.0 - sweep)));
    })
}

fn flip<T: View>(bench: &mut Bench, frames: u64, view: impl FnOnce(Signal<bool>) -> T) -> Outcome {
    let on = bench.mount(|| {
        let on = signal(false);
        (view(on), on)
    })?;
    bench.frames(frames, |_, frame| on.set(frame.is_multiple_of(2)))
}

fn scroll_by(bench: &mut Bench, pointer: Point, frames: u64) -> Outcome {
    bench.frames(frames, |ui, _| {
        ui.pointer_moved(pointer);
        ui.scroll(Point::new(0.0, -40.0));
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
