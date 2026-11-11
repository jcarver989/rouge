use crate::control::each;
use crate::prop::{IntoProp, Prop, PropFrom, markers};
use crate::style::{Dirty, Style, Styled};
use crate::tree::ElementId;
use crate::ui::root_font_size;
use crate::view::{BuildCx, Div, View, div};
use rouge_reactive::{Memo, Signal, effect, memo, on_cleanup, signal};
use rouge_render::{Point, Rect, ScrollState, Size};
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;
use std::rc::Rc;

mod geometry;
mod heights;
use geometry::{Keys, LayoutKey, Measurements, Rows, configure_rows, observe_measurements};
use heights::Heights;

type Identity<T> = Box<dyn Fn(usize) -> RowKey<T>>;

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
    rows: Signal<Option<Rows>>,
    bounds: Signal<HashMap<usize, Signal<Rect>>>,
    request: Signal<Option<RowRequest>>,
    top: Memo<usize>,
}

#[derive(Clone, Copy)]
struct RowRequest {
    index: usize,
    align: Align,
    top_offset: f32,
    top_inset: f32,
}

pub fn virtual_list_handle() -> VirtualListHandle {
    let state = signal(ScrollState::default());
    let rows: Signal<Option<Rows>> = signal(None);
    let top = memo(move || {
        let offset = state.get().offset.y;
        rows.with(|rows| {
            rows.as_ref()
                .filter(|rows| rows.version.is_alive())
                .map_or(0, |rows| {
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
        self.request.set(Some(RowRequest {
            index,
            align,
            top_offset: 0.0,
            top_inset: 0.0,
        }));
    }

    pub fn scroll_to_row_offset(self, index: usize, offset: f32) {
        self.request.set(Some(RowRequest {
            index,
            align: Align::Top,
            top_offset: offset,
            top_inset: 0.0,
        }));
    }

    pub fn reveal_row(self, index: usize, top_inset: f32) {
        self.request.set(Some(RowRequest {
            index,
            align: Align::Visible,
            top_offset: 0.0,
            top_inset,
        }));
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
        self.rows.with(|rows| {
            rows.as_ref()
                .filter(|rows| rows.version.is_alive())
                .map_or(0.0, |rows| rows.with(|rows| rows.top(index)))
        })
    }

    pub fn row_bounds(self, index: usize) -> Option<Rect> {
        self.bounds
            .with(|bounds| bounds.get(&index).map(|bounds| bounds.get()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RowKey<T> {
    pub id: T,
    pub revision: u64,
}

impl<T> RowKey<T> {
    pub fn new(id: T, revision: u64) -> Self {
        Self { id, revision }
    }
}

pub struct VirtualList<T, U = usize> {
    container: Div,
    count: Box<dyn Fn() -> usize>,
    row_height: Prop<RowHeight>,
    row: Rc<dyn Fn(usize) -> T>,
    identity: Option<Identity<U>>,
    measure: Option<Box<dyn Fn() -> u64>>,
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
        identity: None,
        measure: None,
        overscan: OVERSCAN,
        state: None,
        visible: None,
        handle: None,
    }
}

impl<T, U> VirtualList<T, U> {
    pub fn row_identity<V>(
        self,
        identity: impl Fn(usize) -> RowKey<V> + 'static,
    ) -> VirtualList<T, V> {
        VirtualList {
            container: self.container,
            count: self.count,
            row_height: self.row_height,
            row: self.row,
            identity: Some(Box::new(identity)),
            measure: self.measure,
            overscan: self.overscan,
            state: self.state,
            visible: self.visible,
            handle: self.handle,
        }
    }

    pub fn measure_rows(mut self, layout_revision: impl Fn() -> u64 + 'static) -> Self {
        self.measure = Some(Box::new(layout_revision));
        self
    }

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

impl<T: View, U: Clone + Eq + Hash + 'static> Scrollable for VirtualList<T, U> {
    fn scroll_state(mut self, state: Signal<ScrollState>) -> Self {
        self.state = Some(state);
        self
    }
}

impl<T, U> Styled for VirtualList<T, U> {
    fn style_mut(&mut self) -> &mut Style {
        self.container.style_mut()
    }

    fn prop<V: 'static, W>(
        mut self,
        value: impl IntoProp<V, W>,
        dirty: Dirty,
        set: fn(&mut Style, V),
    ) -> Self {
        self.container = self.container.prop(value, dirty, set);
        self
    }
}

impl<T: View, U: Clone + Eq + Hash + 'static> View for VirtualList<T, U> {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (row, overscan) = (self.row, self.overscan);
        let count = memo(self.count);
        let keys = self.identity.map(|identity| {
            memo(move || Rc::new(Keys::new((0..count.get()).map(identity.as_ref()).collect())))
        });
        let handle = self.handle;
        let state = self
            .state
            .or_else(|| handle.map(|handle| handle.state))
            .unwrap_or_else(|| signal(ScrollState::default()));
        let rows = Rows::new();
        let mounted: Option<Measurements<U>> =
            self.measure.as_ref().map(|_| signal(HashMap::new()));
        let width = signal(Size::ZERO);
        let configured = signal(0_u64);
        configure_rows(
            &rows,
            state,
            count,
            keys,
            self.row_height,
            mounted.is_some(),
            configured,
        );
        let layout = self.measure.map(|revision| {
            let font = root_font_size();
            memo(move || (width.get().width, font.get(), revision()))
        });
        if let (Some(mounted), Some(layout)) = (mounted, layout) {
            observe_measurements(&rows, state, keys, configured, mounted, layout);
        }
        if let Some(handle) = handle {
            attach_handle(handle, &rows, state, mounted, layout, keys);
        }
        let visible = {
            let rows = rows.clone();
            memo(move || {
                let current = state.get();
                if current.viewport.height <= 0.0 || current.viewport.width <= 0.0 {
                    return 0..0;
                }
                rows.with(|heights| {
                    let first = heights.item_at(current.offset.y);
                    let last =
                        heights.starting_before(current.offset.y + current.viewport.height, false);
                    first.min(last)..last
                })
            })
        };
        if let Some(tracked) = self.visible {
            effect(move || {
                tracked.set_if_changed(visible.get());
            });
        }
        let range = {
            let rows = rows.clone();
            memo(move || {
                let visible = visible.get();
                if visible.is_empty() {
                    return (0, 0);
                }
                let count = rows.with(Heights::count);
                let start = (visible.start / overscan * overscan).saturating_sub(overscan);
                let end = visible
                    .end
                    .div_ceil(overscan)
                    .saturating_add(1)
                    .saturating_mul(overscan);
                (start, end.min(count))
            })
        };
        let top = {
            let rows = rows.clone();
            move || rows.with(|heights| heights.top(range.get().0))
        };
        let items = {
            let rows = rows.clone();
            memo(move || {
                let (start, end) = range.get();
                let keys = keys.map(|keys| keys.get());
                rows.with(|heights| {
                    let mut items = Vec::new();
                    let mut index = heights.next_visible(start);
                    while index < end {
                        let key = keys.as_ref().and_then(|keys| keys.rows.get(index)).cloned();
                        items.push((index, key));
                        index = heights.next_visible(index + 1);
                    }
                    items
                })
            })
        };
        let row_heights = rows.clone();
        let window = div()
            .absolute()
            .left(0.0)
            .right(0.0)
            .top(top)
            .layer()
            .flex()
            .col()
            .child(each(
                move || items.get(),
                Clone::clone,
                move |item| {
                    let (index, key) = item.get_untracked();
                    let mut container = div().flex_none();
                    if let Some(mounted) = mounted {
                        let size = signal(Size::ZERO);
                        mounted.update(|rows| {
                            rows.insert(index, (key, size));
                        });
                        on_cleanup(move || {
                            if mounted.is_alive() {
                                mounted.update(|rows| {
                                    if rows
                                        .get(&index)
                                        .is_some_and(|(_, current)| *current == size)
                                    {
                                        rows.remove(&index);
                                    }
                                });
                            }
                        });
                        container = container.min_h(1.0);
                        container.spec.size_observer = Some(size);
                    } else {
                        let rows = row_heights.clone();
                        let height = memo(move || {
                            rows.with(|heights| {
                                if index < heights.count() {
                                    heights.height(index)
                                } else {
                                    0.0
                                }
                            })
                        });
                        container = container.h(height);
                    }
                    if let Some(handle) = handle {
                        container = track_row_bounds(container, handle, index);
                    }
                    container.child(row(index))
                },
            ));
        let mut content = div()
            .relative()
            .h(move || rows.with(|heights| heights.top(heights.count())))
            .child(window);
        if mounted.is_some() {
            content.spec.size_observer = Some(width);
        }
        self.container.scroll_state(state).child(content).build(cx)
    }
}

fn attach_handle<T: Clone + Eq + Hash + 'static>(
    handle: VirtualListHandle,
    rows: &Rows,
    state: Signal<ScrollState>,
    mounted: Option<Measurements<T>>,
    layout: Option<Memo<LayoutKey>>,
    keys: Option<Memo<Rc<Keys<T>>>>,
) {
    handle.rows.set(Some(rows.clone()));
    let version = rows.version;
    on_cleanup(move || {
        if handle.rows.is_alive()
            && handle
                .rows
                .with_untracked(|rows| rows.as_ref().is_some_and(|rows| rows.version == version))
        {
            handle.rows.set(None);
        }
    });
    if state != handle.state {
        effect(move || {
            handle.state.set_if_changed(state.get());
        });
        effect(move || {
            let offset = handle.state.get().offset;
            let mut current = state.get_untracked();
            current.offset = offset;
            state.set_if_changed(current);
        });
    }
    let viewport = memo(move || state.get().viewport);
    let rows = rows.clone();
    effect(move || {
        let Some(RowRequest {
            index,
            align,
            top_offset,
            top_inset,
        }) = handle.request.get()
        else {
            return;
        };
        let viewport = viewport.get();
        if viewport.height <= 0.0 || viewport.width <= 0.0 {
            return;
        }
        let current = state.get_untracked();
        let target = rows.with(|rows| {
            if rows.count() == 0 {
                return None;
            }
            let index = index.min(rows.count() - 1);
            let (top, bottom) = (rows.top(index), rows.top(index + 1));
            let visible_top =
                current.offset.y + top_inset.clamp(0.0, (viewport.height - 1.0).max(0.0));
            let y = match align {
                Align::Top => top + top_offset.clamp(0.0, (bottom - top).max(0.0)),
                Align::Center => (top + bottom - viewport.height) / 2.0,
                Align::Bottom => bottom - viewport.height,
                Align::Visible
                    if top < visible_top && bottom >= current.offset.y + viewport.height =>
                {
                    current.offset.y
                }
                Align::Visible if top < visible_top => top - (visible_top - current.offset.y),
                Align::Visible if bottom > current.offset.y + viewport.height => {
                    bottom - viewport.height
                }
                Align::Visible => current.offset.y,
            };
            let max = (rows.top(rows.count()) - viewport.height).max(0.0);
            let measured = match rows {
                Heights::Uniform { .. } => None,
                Heights::Variable(heights) => heights.measurements().get(&index).copied(),
            };
            Some((index, y.clamp(0.0, max), measured))
        });
        let Some((index, y, height)) = target else {
            handle.request.set(None);
            return;
        };
        let measured = mounted.zip(layout).is_none_or(|(mounted, layout)| {
            let width = layout.get().0;
            let keys = keys.map(|keys| keys.get());
            let expected = keys.as_ref().and_then(|keys| keys.rows.get(index));
            mounted.with(|mounted| {
                mounted.get(&index).is_some_and(|(key, size)| {
                    let size = size.get();
                    key.as_ref() == expected
                        && width > 0.0
                        && size.width == width
                        && height == Some(size.height)
                })
            })
        });
        if measured {
            handle.request.set(None);
        }
        if current.offset.y != y {
            state.update(|state| state.offset.y = y);
        }
    });
}

fn track_row_bounds(container: Div, handle: VirtualListHandle, index: usize) -> Div {
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
    container.track_bounds(bounds)
}
