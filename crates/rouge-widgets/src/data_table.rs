use crate::icon::{Icon, IconName};
use crate::tokens::{ICON_SIZE_SMALL, Pace, ROW_HEIGHT, Radius, Space};
use crate::typography::UiStyles;
use crate::util::{Name, TestId, chrome, focus_ring, labelled};
use rouge_core::access;
use rouge_core::component;
use rouge_core::{
    AnyView, Apply, BuildCx, Cursor, DelegateStyle, Div, DragPhase, Key, Prop, Role, RowKey,
    StyleDelta, Styled, View, div, text, view, virtual_list, virtual_list_handle,
};
use rouge_reactive::{Callback, KeyedList, Signal, signal, untrack};
use std::hash::Hash;
use std::marker::PhantomData;
use std::rc::Rc;

pub trait TableRows<T>: Clone + 'static {
    type Key: Clone + Eq + Hash + 'static;

    fn count(&self) -> usize;

    fn key(&self, index: usize) -> Option<Self::Key>;

    fn index_of(&self, key: &Self::Key) -> Option<usize>;

    fn with<U>(&self, index: usize, f: impl FnOnce(&T) -> U) -> Option<U>;
}

impl<T: Eq + Hash + Clone + 'static, U: 'static> TableRows<U> for KeyedList<T, U> {
    type Key = T;

    fn count(&self) -> usize {
        self.keys().with(Vec::len)
    }

    fn key(&self, index: usize) -> Option<T> {
        self.keys().with(|keys| keys.get(index).cloned())
    }

    fn index_of(&self, key: &T) -> Option<usize> {
        self.keys()
            .with_untracked(|keys| keys.iter().position(|other| other == key))
    }

    fn with<V>(&self, index: usize, f: impl FnOnce(&U) -> V) -> Option<V> {
        let key = self
            .keys()
            .with_untracked(|keys| keys.get(index).cloned())?;
        KeyedList::with(self, &key, f)
    }
}

pub struct IndexedRows<T> {
    count: Rc<dyn Fn() -> usize>,
    row: Rc<dyn Fn(usize) -> T>,
}

impl<T> Clone for IndexedRows<T> {
    fn clone(&self) -> Self {
        IndexedRows {
            count: self.count.clone(),
            row: self.row.clone(),
        }
    }
}

pub fn table_rows<T>(
    count: impl Fn() -> usize + 'static,
    row: impl Fn(usize) -> T + 'static,
) -> IndexedRows<T> {
    IndexedRows {
        count: Rc::new(count),
        row: Rc::new(row),
    }
}

impl<T: 'static> TableRows<T> for IndexedRows<T> {
    type Key = usize;

    fn count(&self) -> usize {
        (self.count)()
    }

    fn key(&self, index: usize) -> Option<usize> {
        (index < self.count()).then_some(index)
    }

    fn index_of(&self, key: &usize) -> Option<usize> {
        Some(*key)
    }

    fn with<U>(&self, index: usize, f: impl FnOnce(&T) -> U) -> Option<U> {
        (index < untrack(|| self.count())).then(|| f(&(self.row)(index)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub column: usize,
    pub descending: bool,
}

pub struct Column<T> {
    title: String,
    cell: Rc<dyn Fn(&T) -> AnyView>,
    flex: f32,
    width: Option<Signal<f32>>,
    sortable: bool,
}

pub fn column<T, U: View>(title: impl Into<String>, cell: impl Fn(&T) -> U + 'static) -> Column<T> {
    Column {
        title: title.into(),
        cell: Rc::new(move |row| cell(row).into_any()),
        flex: 1.0,
        width: None,
        sortable: false,
    }
}

impl<T> Column<T> {
    pub fn flex(mut self, flex: f32) -> Self {
        self.flex = flex;
        self
    }

    pub fn width(mut self, width: Signal<f32>) -> Self {
        self.width = Some(width);
        self
    }

    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    fn sized(&self, cell: Div) -> Div {
        let cell = cell.min_w(0.0).overflow_hidden();
        match self.width {
            Some(width) => cell.flex_none().w(move || width.get()),
            None => cell.flex_grow(self.flex).flex_shrink(1.0).flex_basis(0.0),
        }
    }
}

pub struct DataTable<T, U: TableRows<T>> {
    rows: U,
    columns: Vec<Column<T>>,
    selection: Option<Signal<Option<U::Key>>>,
    sort: Option<Signal<Option<Sort>>>,
    on_open: Callback<(U::Key,)>,
    name: Name,
    test_id: Option<String>,
    style: StyleDelta,
    row: PhantomData<T>,
}

#[component]
pub fn data_table<T, U: TableRows<T>>(rows: U) -> DataTable<T, U> {
    DataTable {
        rows,
        columns: Vec::new(),
        selection: None,
        sort: None,
        on_open: Callback::default(),
        name: None,
        test_id: None,
        style: StyleDelta::default(),
        row: PhantomData,
    }
}

impl<T, U: TableRows<T>> DataTable<T, U> {
    pub fn column(mut self, column: Column<T>) -> Self {
        self.columns.push(column);
        self
    }

    pub fn selection(mut self, selection: Signal<Option<U::Key>>) -> Self {
        self.selection = Some(selection);
        self
    }

    pub fn sort(mut self, sort: Signal<Option<Sort>>) -> Self {
        self.sort = Some(sort);
        self
    }

    pub fn on_open(mut self, on_open: impl Into<Callback<(U::Key,)>>) -> Self {
        self.on_open = on_open.into();
        self
    }
}

chrome!(impl[T, U: TableRows<T>] DataTable<T, U>);
labelled!(impl[T, U: TableRows<T>] DataTable<T, U>);

impl<T: 'static, U: TableRows<T>> View for DataTable<T, U> {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (rows, on_open, id) = (self.rows, self.on_open, self.test_id);
        let selection = self.selection.unwrap_or_else(|| signal(None));
        let sort = self.sort.unwrap_or_else(|| signal(None));
        let columns: Rc<[Column<T>]> = self.columns.into();
        let part = |name: String| id.part(format_args!("{name}"));
        let headers = columns.iter().enumerate().map(|(index, column)| {
            let heading = header(column, index, sort)
                .when_some(part(format!("column-{index}")), Div::test_id);
            let grip = column
                .width
                .map(|width| grip(width).when_some(part(format!("resize-{index}")), Div::test_id));
            column.sized(view! {
                <div relative flex items_center>
                    {heading}
                    {grip}
                </div>
            })
        });
        let head = view! {
            <div flex flex_none items_center gap=Space::X2 h=ROW_HEIGHT px=Space::X2 border_b=1.0
                 border_color=Role::BorderSubtle bg=Role::Chrome>
                {..headers}
            </div>
        };
        let scroller = virtual_list_handle();
        let body = {
            let (rows, columns, id) = (rows.clone(), columns.clone(), id.clone());
            view! {
                <virtual_list
                     count={{
                         let rows = rows.clone();
                         move || rows.count()
                     }}
                     row_height=ROW_HEIGHT let:index>
                    {
                        let key = untrack(|| rows.key(index));
                        let cells = rows.with(index, |row| {
                            columns
                                .iter()
                                .map(|column| column.sized(view! { <div>{(column.cell)(row)}</div> }))
                                .collect::<Vec<_>>()
                        });
                        let (chosen, picked, opened) = (key.clone(), key.clone(), key);
                        let marked = chosen.clone();
                        view! {
                            <div role=access::Role::Row
                                 is_selected={move || marked.is_some() && selection.with(|held| *held == marked)}
                                 flex items_center gap=Space::X2 h=ROW_HEIGHT px=Space::X2 text_color=Role::Text
                                 on_click={move |event| {
                                     selection.set(picked.clone());
                                     if let Some(key) = opened.clone().filter(|_| event.click_count == 2) {
                                         on_open.call(key);
                                     }
                                 }}>
                                {cells}
                            </div>
                        }
                        .hover(|style| style.bg(Role::SurfaceHover))
                        .when(
                            move || chosen.is_some() && selection.with(|held| *held == chosen),
                            |style| {
                                style
                                    .bg(Role::Accent)
                                    .text_color(Role::AccentForeground)
                            },
                        )
                        .when_some(
                            id.part(format_args!("row-{index}")),
                            Div::test_id,
                        )
                    }
                </virtual_list>
            }
        };
        let body = {
            let rows = rows.clone();
            body.row_identity(move |index| RowKey::new(rows.key(index), 0))
                .handle(scroller)
                .flex_1()
                .min_h(0.0)
        };
        let step = {
            let rows = rows.clone();
            move |to: &dyn Fn(Option<usize>, usize) -> usize| {
                let end = untrack(|| rows.count()).saturating_sub(1);
                let current = selection
                    .with_untracked(|held| held.as_ref().and_then(|key| rows.index_of(key)));
                let next = to(current, end).min(end);
                selection.set(untrack(|| rows.key(next)));
                scroller.reveal_row(next, 0.0);
            }
        };
        let root = view! {
            <div flex col min_h=0.0 border=1.0 border_color=Role::BorderSubtle rounded=Radius::Control
                 overflow_hidden focusable
                 on_key_down={move |event| {
                     match event.key {
                         Key::ArrowDown => step(&|held, _| held.map_or(0, |at| at + 1)),
                         Key::ArrowUp => step(&|held, _| held.map_or(0, |at| at.saturating_sub(1))),
                         Key::Home => step(&|_, _| 0),
                         Key::End => step(&|_, last| last),
                         Key::Enter => {
                             if let Some(key) = selection.get_untracked() {
                                 on_open.call(key);
                             }
                         }
                         _ => return,
                     }
                     event.stop_propagation();
                 }}>
                {head}
                {body}
            </div>
        };
        let name = self
            .name
            .unwrap_or_else(|| Prop::Static("Table".to_owned()));
        let root = root.access_label(name).role(access::Role::Table);
        self.style
            .apply(focus_ring(root).when_some(id, Div::test_id))
            .build(cx);
    }
}

fn header<T>(column: &Column<T>, index: usize, sort: Signal<Option<Sort>>) -> Div {
    let label = view! { <text label secondary no_wrap>{column.title.clone()}</text> };
    let heading = view! { <div flex items_center gap=Space::X1 min_w=0.0>{label}</div> };
    if !column.sortable {
        return heading;
    }
    let active = move || sort.with(|sort| sort.filter(|sort| sort.column == index));
    let arrow = view! {
        <div flex_none opacity={move || if active().is_some() { 1.0 } else { 0.0 }}
             rotate={move || {
                 if active().is_some_and(|sort| sort.descending) {
                     180.0
                 } else {
                     0.0
                 }
             }}
             transition=Pace::Move>
            {Icon::new(IconName::ArrowUp, Role::TextSecondary).size(ICON_SIZE_SMALL)}
        </div>
    };
    heading
        .child(arrow)
        .role(access::Role::ColumnHeader)
        .on_activate(move || {
            sort.update(|sort| {
                let descending = sort.is_some_and(|sort| sort.column == index && !sort.descending);
                *sort = Some(Sort {
                    column: index,
                    descending,
                });
            });
        })
}

fn grip(width: Signal<f32>) -> Div {
    let origin = signal(0.0);
    view! {
        <div absolute top=0.0 bottom=0.0 right=0.0 w=GRIP cursor=Cursor::ResizeHorizontal
             on_drag={move |event| match event.phase {
                 DragPhase::Start => origin.set(width.get_untracked()),
                 DragPhase::Move => {
                     width.set((origin.get_untracked() + event.translation.x).max(MIN_COLUMN));
                 }
                 DragPhase::End => {}
             }}/>
    }
}

const GRIP: f32 = 8.0;
const MIN_COLUMN: f32 = 40.0;
