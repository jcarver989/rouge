use crate::control::each;
use crate::prop::{IntoProp, Prop, PropFrom, markers};
use crate::style::Styled;
use crate::styled;
use crate::tree::ElementId;
use crate::view::{BuildCx, Div, View, div};
use rouge_reactive::{Memo, Signal, effect, memo, on_cleanup, signal};
use rouge_render::{Point, Rect, ScrollState};
use std::cell::RefCell;
use std::collections::HashMap;
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
enum Heights {
    Uniform(f32, usize),
    Each(Vec<Memo<f32>>),
}

#[derive(Clone, PartialEq)]
enum Rows {
    Uniform(f32, usize),
    Tops(Vec<f32>),
}

impl Rows {
    fn new(heights: &Heights) -> Rows {
        match heights {
            Heights::Uniform(height, count) => Rows::Uniform(*height, *count),
            Heights::Each(heights) => {
                let tops = heights.iter().scan(0.0, |top, height| {
                    *top += height.get().max(0.0);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Top,
    Center,
    Bottom,
    Visible,
}

#[derive(Clone, Copy)]
pub struct VirtualListHandle {
    state: Signal<ScrollState>,
    rows: Signal<Option<Memo<Rows>>>,
    bounds: Signal<HashMap<usize, Signal<Rect>>>,
    request: Signal<Option<(usize, Align)>>,
    top: Memo<usize>,
}

pub fn virtual_list_handle() -> VirtualListHandle {
    let state = signal(ScrollState::default());
    let rows: Signal<Option<Memo<Rows>>> = signal(None);
    let top = memo(move || {
        let offset = state.get().offset.y;
        rows.with(|rows| {
            rows.map_or(0, |rows| {
                rows.with(|rows| {
                    rows.starting_before(offset, false)
                        .min(rows.count().saturating_sub(1))
                })
            })
        })
    });
    VirtualListHandle {
        state,
        rows,
        bounds: signal(HashMap::new()),
        request: signal(None),
        top,
    }
}

impl VirtualListHandle {
    pub fn scroll_to_row(self, index: usize, align: Align) {
        self.request.set(Some((index, align)));
    }

    pub fn scroll_to(self, offset: Point) {
        self.request.set(None);
        self.state.update(|state| state.scroll_to(offset));
    }

    pub fn scroll_state(self) -> Signal<ScrollState> {
        self.state
    }

    pub fn top_row(self) -> Memo<usize> {
        self.top
    }

    pub fn row_top(self, index: usize) -> f32 {
        self.rows
            .with(|rows| rows.map_or(0.0, |rows| rows.with(|rows| rows.top(index))))
    }

    pub fn row_bounds(self, index: usize) -> Option<Rect> {
        self.bounds
            .with(|bounds| bounds.get(&index).map(|bounds| bounds.get()))
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
    handle: Option<VirtualListHandle>,
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
        handle: None,
    }
}

impl<T> VirtualList<T> {
    pub fn handle(mut self, handle: VirtualListHandle) -> Self {
        self.handle = Some(handle);
        self
    }

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
        let handle = self.handle;
        let state = self
            .state
            .or_else(|| handle.map(|handle| handle.state))
            .unwrap_or_else(|| signal(ScrollState::default()));
        let heights = memo(move || {
            let count = count();
            match row_height.get() {
                RowHeight::Uniform(height) => Heights::Uniform(height.max(0.001), count),
                RowHeight::Each(height) => Heights::Each(
                    (0..count)
                        .map(|index| {
                            let height = height.clone();
                            memo(move || height(index))
                        })
                        .collect(),
                ),
            }
        });
        let rows = memo(move || heights.with(Rows::new));
        let previous: RefCell<Option<Rows>> = RefCell::new(None);
        effect(move || {
            let current = rows.get();
            if let Some(previous) = previous.replace(Some(current.clone())) {
                let offset = state.get_untracked().offset;
                let index = previous
                    .starting_before(offset.y, true)
                    .saturating_sub(1)
                    .min(current.count().saturating_sub(1));
                let y = current.top(index) + offset.y - previous.top(index);
                state.update(|state| state.offset.y = y.max(0.0));
            }
        });
        if let Some(handle) = handle {
            handle.rows.set(Some(rows));
            if state != handle.state {
                effect(move || {
                    handle.state.set_if_changed(state.get());
                });
                effect(move || {
                    let offset = handle.state.get().offset;
                    state.update(|state| state.offset = offset);
                });
            }
            let viewport = memo(move || state.get().viewport);
            effect(move || {
                let request = handle.request.get();
                let viewport = viewport.get();
                if let Some((index, align)) = request.filter(|_| viewport.height > 0.0) {
                    handle.request.set(None);
                    let current = state.get_untracked();
                    rows.with(|rows| {
                        if rows.count() == 0 {
                            return;
                        }
                        let index = index.min(rows.count() - 1);
                        let (top, bottom) = (rows.top(index), rows.top(index + 1));
                        let y = match align {
                            Align::Top => top,
                            Align::Center => (top + bottom - viewport.height) / 2.0,
                            Align::Bottom => bottom - viewport.height,
                            Align::Visible if top < current.offset.y => top,
                            Align::Visible if bottom > current.offset.y + viewport.height => {
                                bottom - viewport.height
                            }
                            Align::Visible => current.offset.y,
                        };
                        let max = (rows.top(rows.count()) - viewport.height).max(0.0);
                        state.update(|state| state.offset.y = y.clamp(0.0, max));
                    });
                }
            });
        }
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
                    let height = memo(move || top(index + 1) - top(index));
                    let mut container = div().h(height);
                    if let Some(handle) = handle {
                        let bounds = signal(Rect::ZERO);
                        handle.bounds.update(|mounted| {
                            mounted.insert(index, bounds);
                        });
                        on_cleanup(move || {
                            if handle.bounds.is_alive() {
                                handle.bounds.update(|mounted| {
                                    if mounted.get(&index) == Some(&bounds) {
                                        mounted.remove(&index);
                                    }
                                });
                            }
                        });
                        container = container.track_bounds(bounds);
                    }
                    container.child(row(index))
                },
            ));
        let content = div()
            .relative()
            .h(move || top(rows.with(Rows::count)))
            .child(window);
        self.container.scroll_state(state).child(content).build(cx)
    }
}
