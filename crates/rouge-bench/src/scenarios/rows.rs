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
            for id in self
                .rows
                .keys()
                .get_untracked()
                .iter()
                .step_by(UPDATED_STEP)
            {
                self.rows.update(id, |record| record.label.push_str(SUFFIX));
            }
        });
    }

    fn swap(self) {
        let first = SWAPPED_ROW;
        let Some(last) = self.size.checked_sub(2) else {
            return;
        };
        let pair = self
            .rows
            .order()
            .with_untracked(|order| Some((*order.get(first)?, *order.get(last)?)));
        if let Some((low, high)) = pair {
            batch(|| {
                self.rows.move_to(&low, last);
                self.rows.move_to(&high, first);
            });
        }
    }

    fn check(self, op: Operation, first: u64) {
        let size = self.size;
        let key = |index| {
            self.rows
                .order()
                .with_untracked(|order| order.get(index).copied())
        };
        match op {
            Operation::Create | Operation::Replace => {
                assert_eq!(self.rows.len(), size);
                assert_eq!(key(0), Some(first));
            }
            Operation::Append => {
                assert_eq!(self.rows.len(), 2 * size);
                assert_eq!(key(size), Some(first + size as u64));
            }
            Operation::Clear => assert!(self.rows.is_empty()),
            Operation::Update => assert_eq!(
                self.rows.with(&first, |row| row.label.ends_with(SUFFIX)),
                Some(true)
            ),
            Operation::Select => assert_eq!(
                self.selected.get_untracked(),
                Some(first + SELECTED_ROW as u64)
            ),
            Operation::Remove => {
                assert_eq!(self.rows.len(), size - 1);
                assert!(
                    self.rows
                        .with(&(first + REMOVED_ROW as u64), |_| ())
                        .is_none()
                );
            }
            Operation::Swap => {
                assert_eq!(key(SWAPPED_ROW), Some(first + (size - 2) as u64));
                assert_eq!(key(size - 2), Some(first + SWAPPED_ROW as u64));
            }
        }
    }
}

pub(super) fn run(
    bench: &mut Bench,
    operation: Operation,
    virtualized: bool,
) -> Result<(), BenchError> {
    let size = bench.size();
    let samples = (SAMPLED_ROWS / size).clamp(FEWEST_SAMPLES, SAMPLES);
    let target = operation.target();
    let table = bench.mount(|| {
        let table = Table::new(size);
        (page(table, virtualized), table)
    })?;
    for sample in 0..WARMUPS + samples {
        let first = bench.setup(|ui| {
            ui.pointer_moved(target);
            let first = table.next.get_untracked();
            table.replace(operation.initial(size));
            if matches!(operation, Operation::Select) {
                table.selected.set(Some(first + PREVIOUS_ROW as u64));
            }
            if matches!(operation, Operation::Replace) {
                table.next.get_untracked()
            } else {
                first
            }
        })?;
        if sample < WARMUPS {
            bench.setup(|ui| ui.click(target))?;
        } else {
            bench.measure(|ui| ui.click(target))?;
        }
        untrack(|| table.check(operation, first));
    }
    Ok(())
}

pub fn row_table(size: usize) -> impl View {
    page(Table::new(size), false)
}

fn page(table: Table, virtualized: bool) -> impl View {
    let selection = selector(move || table.selected.get());
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
    view! {
        <div size_full flex col bg=0x1e1e2e>
            <div flex h=TOOLBAR_HEIGHT>
                {control("Create", move || table.replace(table.size))}
                {control("Append", move || table.append(table.size))}
                {control("Update", move || table.update())}
                {control("Clear", move || table.clear())}
                {control("Swap", move || table.swap())}
            </div>
            {body}
        </div>
    }
}

fn control(label: &'static str, run: impl Fn() + 'static) -> Div {
    view! {
        <div flex items_center w=BUTTON_WIDTH h=TOOLBAR_HEIGHT on_click={move |_| run()}>
            <text color=0xcdd6f4>{label}</text>
        </div>
    }
}

fn row(table: Table, selection: Selector<Option<u64>>, id: u64) -> impl View {
    let on = selection.is(Some(id));
    let label = table.rows.select(id, |record| record.label.clone());
    let background = move || Color::rgb(if on.get() { 0x45475a } else { 0x1e1e2e });
    view! {
        <div flex w_full h=ROW_HEIGHT bg=background>
            <div flex items_center h=ROW_HEIGHT w=ID_WIDTH><text color=0xcdd6f4>{id.to_string()}</text></div>
            <div flex items_center h=ROW_HEIGHT w=LABEL_WIDTH>
                <div flex items_center size_full on_click={move |_| table.selected.set(Some(id))}><text color=0x89b4fa>{move || label.get()}</text></div>
            </div>
            <div flex items_center h=ROW_HEIGHT w=REMOVE_WIDTH>
                <div flex items_center size_full on_click={move |_| { table.rows.remove(&id); }}><text color=0xf38ba8>"x"</text></div>
            </div>
            <div flex items_center h=ROW_HEIGHT flex_1/>
        </div>
    }
}

fn label(seed: usize) -> String {
    let mixed = seed.wrapping_mul(2_654_435_761) >> 7;
    let adjective = ADJECTIVES[mixed % ADJECTIVES.len()];
    let color = COLORS[mixed / 16 % COLORS.len()];
    let noun = NOUNS[mixed / 176 % NOUNS.len()];
    format!("{adjective} {color} {noun}")
}
