use crate::bench::Bench;
use crate::error::BenchError;
use rouge::prelude::*;

const WARMUPS: usize = 5;
const SAMPLES: usize = 25;
const SAMPLED_ROWS: usize = 50_000;
const FEWEST_SAMPLES: usize = 5;
const TOOLBAR_HEIGHT: f32 = 48.0;
const BUTTON_WIDTH: f32 = 120.0;
const ROW_HEIGHT: f32 = 36.0;
const ID_WIDTH: f32 = 80.0;
const LABEL_WIDTH: f32 = 320.0;
const REMOVE_WIDTH: f32 = 80.0;
const SELECTED_ROW: usize = 1;
const PREVIOUS_ROW: usize = 4;
const REMOVED_ROW: usize = 3;
const SWAPPED_ROW: usize = 1;
const UPDATED_STEP: usize = 10;
const SUFFIX: &str = " !!!";

const ADJECTIVES: [&str; 16] = [
    "quiet", "bright", "narrow", "gentle", "sturdy", "hollow", "polite", "rapid", "curious",
    "frosty", "mellow", "ancient", "modern", "clumsy", "elegant", "plain",
];
const COLORS: [&str; 11] = [
    "red", "amber", "green", "teal", "blue", "violet", "pink", "brown", "gray", "black", "white",
];
const NOUNS: [&str; 13] = [
    "kettle", "lantern", "bicycle", "ladder", "blanket", "compass", "guitar", "window", "basket",
    "teapot", "carpet", "mirror", "pencil",
];

#[derive(Clone, Copy, Debug)]
pub(super) enum Operation {
    Create,
    Append,
    Update,
    Clear,
    Swap,
    Replace,
    Select,
    Remove,
}

impl Operation {
    pub(super) const fn minimum(self) -> usize {
        match self {
            Operation::Select => PREVIOUS_ROW + 1,
            Operation::Remove => REMOVED_ROW + 1,
            Operation::Swap => SWAPPED_ROW + 3,
            _ => 1,
        }
    }

    fn initial(self, size: usize) -> usize {
        if matches!(self, Operation::Create) {
            0
        } else {
            size
        }
    }

    fn target(self) -> Point {
        let button =
            |index: usize| Point::new(BUTTON_WIDTH * (index as f32 + 0.5), TOOLBAR_HEIGHT / 2.0);
        let cell =
            |row: usize, x: f32| Point::new(x, TOOLBAR_HEIGHT + ROW_HEIGHT * (row as f32 + 0.5));
        match self {
            Operation::Create | Operation::Replace => button(0),
            Operation::Append => button(1),
            Operation::Update => button(2),
            Operation::Clear => button(3),
            Operation::Swap => button(4),
            Operation::Select => cell(SELECTED_ROW, ID_WIDTH + LABEL_WIDTH / 2.0),
            Operation::Remove => cell(REMOVED_ROW, ID_WIDTH + LABEL_WIDTH + REMOVE_WIDTH / 2.0),
        }
    }

    fn expected(self, table: Table, before: &[u64]) -> Vec<u64> {
        let next = table.next.get_untracked();
        let added = next..next + table.size as u64;
        let mut keys = before.to_vec();
        match self {
            Operation::Create | Operation::Replace => keys = added.collect(),
            Operation::Append => keys.extend(added),
            Operation::Update | Operation::Select => {}
            Operation::Clear => keys.clear(),
            Operation::Swap => keys.swap(SWAPPED_ROW, table.size - 2),
            Operation::Remove => {
                keys.remove(REMOVED_ROW);
            }
        }
        keys
    }
}

struct Record {
    id: u64,
    label: String,
}

#[derive(Clone, Copy)]
struct Table {
    size: usize,
    rows: KeyedList<u64, Record>,
    selected: Signal<Option<u64>>,
    next: Signal<u64>,
}

impl Table {
    fn new(size: usize) -> Self {
        Table {
            size,
            rows: keyed_list(|record: &Record| record.id),
            selected: signal(None),
            next: signal(1),
        }
    }

    fn keys(self) -> Vec<u64> {
        self.rows.keys().get_untracked()
    }

    fn append(self, count: usize) {
        let first = self.next.get_untracked();
        let count = count as u64;
        self.next.set(first + count);
        let record = |id| Record {
            id,
            label: label(id as usize),
        };
        self.rows.extend((first..first + count).map(record));
    }

    fn replace(self, count: usize) {
        batch(|| {
            self.clear();
            self.append(count);
        });
    }

    fn clear(self) {
        batch(|| {
            self.rows.retain(|_| false);
            self.selected.set(None);
        });
    }

    fn update(self) {
        batch(|| {
            for id in self.keys().iter().step_by(UPDATED_STEP) {
                self.rows.update(id, |record| record.label.push_str(SUFFIX));
            }
        });
    }

    fn swap(self) {
        let (first, last) = (SWAPPED_ROW, self.size - 2);
        let keys = self.keys();
        if let (Some(low), Some(high)) = (keys.get(first), keys.get(last)) {
            batch(|| {
                self.rows.move_to(low, last);
                self.rows.move_to(high, first);
            });
        }
    }

    fn check(self, operation: Operation, expected: &[u64]) {
        assert_eq!(self.keys(), expected, "{operation:?}");
        let labels = self.rows.with_values(|rows| {
            let suffixed = rows.iter().filter(|row| row.label.ends_with(SUFFIX));
            suffixed.count()
        });
        let updated = match operation {
            Operation::Update => expected.len().div_ceil(UPDATED_STEP),
            _ => 0,
        };
        assert_eq!(labels, updated, "{operation:?}");
        let selected = match operation {
            Operation::Select => expected.get(SELECTED_ROW).copied(),
            _ => None,
        };
        assert_eq!(self.selected.get_untracked(), selected, "{operation:?}");
    }
}

pub(super) fn run(bench: &mut Bench, operation: Operation) -> Result<(), BenchError> {
    measure(bench, operation, false)
}

pub(super) fn virtual_create(bench: &mut Bench) -> Result<(), BenchError> {
    measure(bench, Operation::Create, true)
}

fn measure(bench: &mut Bench, operation: Operation, virtualized: bool) -> Result<(), BenchError> {
    let size = bench.size();
    let samples = (SAMPLED_ROWS / size).clamp(FEWEST_SAMPLES, SAMPLES);
    let target = operation.target();
    let table = bench.mount(|| {
        let table = Table::new(size);
        (page(table, virtualized), table)
    })?;
    for sample in 0..WARMUPS + samples {
        let expected = bench.setup(|ui| {
            ui.pointer_moved(target);
            table.replace(operation.initial(size));
            let before = table.keys();
            if matches!(operation, Operation::Select) {
                table.selected.set(before.get(PREVIOUS_ROW).copied());
            }
            operation.expected(table, &before)
        })?;
        if sample < WARMUPS {
            bench.setup(|ui| ui.click(target))?;
        } else {
            bench.measure(|ui| ui.click(target))?;
        }
        untrack(|| table.check(operation, &expected));
    }
    Ok(())
}

fn page(table: Table, virtualized: bool) -> impl View {
    let selection = selector(move || table.selected.get());
    let toolbar = div()
        .flex()
        .h(TOOLBAR_HEIGHT)
        .child(control("Create", move || table.replace(table.size)))
        .child(control("Append", move || table.append(table.size)))
        .child(control("Update", move || table.update()))
        .child(control("Clear", move || table.clear()))
        .child(control("Swap", move || table.swap()));
    let body = if virtualized {
        virtual_list(table.rows, ROW_HEIGHT, move |index| {
            let id = table
                .rows
                .order()
                .with_untracked(|order| *order.get(index).expect("visible row has a key"));
            row(table, selection, id)
        })
        .flex_1()
        .min_h(0.0)
        .into_any()
    } else {
        let rows = each(
            table.rows.keys(),
            |id| *id,
            move |id: ReadSignal<u64>| row(table, selection, id.get_untracked()),
        );
        scroll().flex_1().min_h(0.0).child(rows).into_any()
    };
    div()
        .size_full()
        .flex()
        .col()
        .bg(0x1e1e2e)
        .child(toolbar)
        .child(body)
}

fn control(label: &'static str, run: impl Fn() + 'static) -> Div {
    div()
        .flex()
        .items_center()
        .w(BUTTON_WIDTH)
        .h(TOOLBAR_HEIGHT)
        .on_click(move |_| run())
        .child(text(label).color(0xcdd6f4))
}

fn row(table: Table, selection: Selector<Option<u64>>, id: u64) -> impl View {
    let on = selection.is(Some(id));
    let label = table.rows.select(id, |record| record.label.clone());
    let cell = || div().flex().items_center().h(ROW_HEIGHT);
    let link = || div().flex().items_center().size_full();
    let select = link()
        .on_click(move |_| table.selected.set(Some(id)))
        .child(text(move || label.get()).color(0x89b4fa));
    let remove = link()
        .on_click(move |_| {
            table.rows.remove(&id);
        })
        .child(text("x").color(0xf38ba8));
    div()
        .flex()
        .w_full()
        .h(ROW_HEIGHT)
        .bg(move || Color::rgb(if on.get() { 0x45475a } else { 0x1e1e2e }))
        .child(
            cell()
                .w(ID_WIDTH)
                .child(text(id.to_string()).color(0xcdd6f4)),
        )
        .child(cell().w(LABEL_WIDTH).child(select))
        .child(cell().w(REMOVE_WIDTH).child(remove))
        .child(cell().flex_1())
}

fn label(seed: usize) -> String {
    let mixed = seed.wrapping_mul(2_654_435_761) >> 7;
    let adjective = ADJECTIVES[mixed % ADJECTIVES.len()];
    let color = COLORS[mixed / 16 % COLORS.len()];
    let noun = NOUNS[mixed / 176 % NOUNS.len()];
    format!("{adjective} {color} {noun}")
}
