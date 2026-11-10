use crate::control::each;
use crate::prop::{IntoProp, Prop, PropFrom, markers};
use crate::style::Styled;
use crate::styled;
use crate::tree::ElementId;
use crate::view::{BuildCx, Div, View, div};
use rouge_reactive::{Signal, effect, memo, signal};
use rouge_render::ScrollState;
use std::ops::Range;
use std::rc::Rc;

const OVERSCAN: usize = 8;

pub fn scroll() -> Div {
    div().scrollable()
}

pub trait Scrollable: View {
    fn scroll_state(self, state: Signal<ScrollState>) -> Self;
}

#[derive(Clone)]
pub enum RowHeight {
    Uniform(f32),
    Each(Rc<dyn Fn(usize) -> f32>),
}

impl PropFrom<f32> for RowHeight {
    fn prop_from(value: f32) -> Self {
        RowHeight::Uniform(value)
    }
}

impl<T: Fn(usize) -> f32 + 'static> IntoProp<RowHeight, markers::Indexed> for T {
    fn into_prop(self) -> Prop<RowHeight> {
        Prop::Static(RowHeight::Each(Rc::new(self)))
    }
}

#[derive(PartialEq)]
enum Rows {
    Uniform(f32, usize),
    Tops(Vec<f32>),
}

impl Rows {
    fn new(height: &RowHeight, count: usize) -> Rows {
        match height {
            RowHeight::Uniform(height) => Rows::Uniform(*height, count),
            RowHeight::Each(height) => {
                let tops = (0..count).scan(0.0, |top, index| {
                    *top += height(index);
                    Some(*top)
                });
                Rows::Tops([0.0].into_iter().chain(tops).collect())
            }
        }
    }

    fn count(&self) -> usize {
        match self {
            Rows::Uniform(_, count) => *count,
            Rows::Tops(tops) => tops.len() - 1,
        }
    }

    fn top(&self, index: usize) -> f32 {
        match self {
            Rows::Uniform(height, _) => index as f32 * height,
            Rows::Tops(tops) => tops[index.min(tops.len() - 1)],
        }
    }

    fn starting_before(&self, y: f32, inclusive: bool) -> usize {
        match self {
            Rows::Uniform(height, _) if inclusive => (y / height).floor().max(0.0) as usize + 1,
            Rows::Uniform(height, _) => (y / height).ceil().max(0.0) as usize,
            Rows::Tops(tops) => tops.partition_point(|top| *top < y || (inclusive && *top == y)),
        }
    }
}

pub struct VirtualList<T> {
    container: Div,
    count: Box<dyn Fn() -> usize>,
    row_height: Prop<RowHeight>,
    row: Rc<dyn Fn(usize) -> T>,
    overscan: usize,
    state: Option<Signal<ScrollState>>,
    visible: Option<Signal<Range<usize>>>,
}

pub fn virtual_list<T: View, U>(
    count: impl Fn() -> usize + 'static,
    row_height: impl IntoProp<RowHeight, U>,
    row: impl Fn(usize) -> T + 'static,
) -> VirtualList<T> {
    VirtualList {
        container: scroll(),
        count: Box::new(count),
        row_height: row_height.into_prop(),
        row: Rc::new(row),
        overscan: OVERSCAN,
        state: None,
        visible: None,
    }
}

impl<T> VirtualList<T> {
    pub fn overscan(mut self, rows: usize) -> Self {
        self.overscan = rows.max(1);
        self
    }

    pub fn track_rows(mut self, visible: Signal<Range<usize>>) -> Self {
        self.visible = Some(visible);
        self
    }
}

impl<T: View> Scrollable for VirtualList<T> {
    fn scroll_state(mut self, state: Signal<ScrollState>) -> Self {
        self.state = Some(state);
        self
    }
}

styled!(VirtualList<T> => container);

impl<T: View> View for VirtualList<T> {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (row, overscan) = (self.row, self.overscan);
        let (count, row_height) = (self.count, self.row_height);
        let state = self.state.unwrap_or_else(|| signal(ScrollState::default()));
        let rows = memo(move || Rows::new(&row_height.get(), count()));
        let visible = memo(move || {
            let state = state.get();
            rows.with(|rows| {
                let first = rows.starting_before(state.offset.y, true).max(1) - 1;
                let last = rows.starting_before(state.offset.y + state.viewport.height, false);
                first.min(rows.count())..last.min(rows.count())
            })
        });
        if let Some(tracked) = self.visible {
            effect(move || tracked.set(visible.get()));
        }
        let range = memo(move || {
            let (visible, count) = (visible.get(), rows.with(Rows::count));
            let start = (visible.start / overscan * overscan).saturating_sub(overscan);
            let end = visible.end.div_ceil(overscan) * overscan + overscan;
            (start.min(count), end.min(count))
        });
        let top = move |index: usize| rows.with(|rows| rows.top(index));
        let window = div()
            .absolute()
            .left(0.0)
            .right(0.0)
            .top(move || top(range.get().0))
            .layer()
            .child(each(
                move || {
                    let (start, end) = range.get();
                    (start..end).collect()
                },
                |index| *index,
                move |index| {
                    let index = index.get_untracked();
                    div()
                        .h(move || top(index + 1) - top(index))
                        .child(row(index))
                },
            ));
        let content = div()
            .relative()
            .h(move || top(rows.with(Rows::count)))
            .child(window);
        self.container.scroll_state(state).child(content).build(cx)
    }
}
