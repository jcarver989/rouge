use super::layers::Slot;
use super::{ElementId, On, Tree};
use crate::event::Cursor;
use crate::selection::TextBreak;
use crate::style::Dirty;
use crate::text::Granularity;
use crate::ui::Wake;
use crate::window::EditAction;
use rouge_reactive::untrack;
use rouge_render::Point;
use std::cmp::Ordering;
use std::iter::from_fn;
use std::mem::take;
use std::ops::Range;
use std::rc::Rc;

const ROW_BREAK: &str = "\n\n";
const AUTOSCROLL_FRAME: f64 = 1.0 / 60.0;
const AUTOSCROLL_BASE: f32 = 240.0;
const AUTOSCROLL_GAIN: f32 = 12.0;
const AUTOSCROLL_MAX: f32 = 6000.0;

pub(crate) struct RowSource {
    pub count: Rc<dyn Fn() -> usize>,
    pub identity: Rc<dyn Fn(usize) -> u64>,
    pub text: Option<Rc<dyn Fn(usize) -> String>>,
}

#[derive(Default)]
pub(crate) struct SelectionScope {
    list: Option<ElementId>,
    range: Option<TextRange>,
    endpoints: Vec<RowSnapshot>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct TextPoint {
    row: usize,
    leaf: usize,
    offset: usize,
}

impl TextPoint {
    const fn new(row: usize, leaf: usize, offset: usize) -> Self {
        Self { row, leaf, offset }
    }

    const fn row_end(row: usize) -> Self {
        Self::new(row, usize::MAX, usize::MAX)
    }

    const fn at(self, offset: usize) -> Self {
        Self { offset, ..self }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct TextRange {
    anchor: TextPoint,
    focus: TextPoint,
}

impl TextRange {
    fn start(self) -> TextPoint {
        self.anchor.min(self.focus)
    }

    fn end(self) -> TextPoint {
        self.anchor.max(self.focus)
    }

    fn visible(self) -> bool {
        self.anchor != self.focus
    }

    fn partial_endpoint(self, row: usize) -> bool {
        (row == self.start().row || row == self.end().row) && !self.whole_row(row)
    }

    fn whole_row(self, row: usize) -> bool {
        self.start() <= TextPoint::new(row, 0, 0) && self.end() >= TextPoint::row_end(row)
    }

    fn clip(self, row: usize, leaf: usize, content: &str) -> Option<Range<usize>> {
        let (start, end) = (self.start(), self.end());
        let len = content.len();
        let (first, last) = (TextPoint::new(row, leaf, 0), TextPoint::new(row, leaf, len));
        if len == 0 {
            return (start < first && end > first).then_some(0..0);
        }
        if end <= first || start >= last {
            return None;
        }
        let from = if start > first {
            content.floor_char_boundary(start.offset.min(len))
        } else {
            0
        };
        let to = if end < last {
            content.floor_char_boundary(end.offset.min(len))
        } else {
            len
        };
        (from < to).then_some(from..to)
    }
}

struct RowSnapshot {
    row: usize,
    identity: u64,
    pieces: Vec<(String, TextBreak)>,
}

#[derive(Clone, Copy)]
enum Unit {
    Caret,
    Word,
    Leaf,
}

#[derive(Clone, Copy)]
pub(super) struct SelectionDrag {
    pub(crate) scope: ElementId,
    scroller: Option<ElementId>,
    start: Point,
    position: Point,
    started: bool,
    unit: Unit,
    origin: (TextPoint, TextPoint),
    ticked: Option<f64>,
}

struct Leaf {
    id: ElementId,
    list: Option<ElementId>,
    row: usize,
    ordinal: usize,
    visible: bool,
}

struct Hit<'a> {
    leaf: &'a Leaf,
    point: TextPoint,
    word: Range<usize>,
    len: usize,
}

impl Hit<'_> {
    fn span(&self, unit: Unit) -> (TextPoint, TextPoint) {
        let point = self.point;
        match unit {
            Unit::Caret => (point, point),
            Unit::Word => (point.at(self.word.start), point.at(self.word.end)),
            Unit::Leaf => (point.at(0), point.at(self.len)),
        }
    }
}

impl Tree {
    fn selection(&self, id: ElementId) -> Option<&SelectionScope> {
        self.elements.get(id)?.extras()?.selection.as_deref()
    }

    fn selection_mut(&mut self, id: ElementId) -> Option<&mut SelectionScope> {
        self.elements
            .get_mut(id)?
            .extras
            .as_mut()?
            .selection
            .as_deref_mut()
    }

    fn scope_in(&self, path: &[ElementId]) -> Option<(usize, ElementId)> {
        path.iter()
            .enumerate()
            .rev()
            .find(|(_, id)| self.selection(**id).is_some())
            .map(|(depth, id)| (depth, *id))
    }

    pub(crate) fn selection_cursor(&self, path: &[ElementId]) -> Option<Cursor> {
        let leaf = self.elements.get(*path.last()?)?;
        if leaf.kind.text().is_none_or(|text| text.edit.is_some()) {
            return None;
        }
        for id in path.iter().rev() {
            let element = self.elements.get(*id)?;
            if element.flags.unselectable || element.cursor.is_some() || element.disabled() {
                return None;
            }
            if self.selection(*id).is_some() {
                return Some(Cursor::Text);
            }
        }
        None
    }

    pub(crate) fn select_edit(&mut self, path: &[ElementId], action: EditAction) -> bool {
        let Some((_, scope)) = self.scope_in(path) else {
            return false;
        };
        match action {
            EditAction::Copy => self.selected_text(scope).is_some_and(|text| {
                self.clipboard.write(&text);
                true
            }),
            EditAction::SelectAll => self.select_all(scope),
            _ => false,
        }
    }

    pub(crate) fn escape_selection(&mut self, path: &[ElementId]) -> bool {
        self.scope_in(path)
            .is_some_and(|(_, scope)| self.clear_selection(scope))
    }

    fn clear_selection(&mut self, scope: ElementId) -> bool {
        let Some(range) = self
            .selection_mut(scope)
            .and_then(|state| state.range.take())
        else {
            return false;
        };
        self.refresh_selection(scope, &self.selection_leaves(scope), false);
        range.visible()
    }

    pub(crate) fn press_selection(
        &mut self,
        path: &[ElementId],
        position: Point,
        clicks: u32,
        extend: bool,
    ) {
        self.selection_drag = None;
        let target = self.scope_in(path);
        for index in 0..self.selection_scopes.len() {
            let scope = self.selection_scopes[index];
            if target.is_none_or(|(_, target)| target != scope) {
                self.clear_selection(scope);
            }
        }
        let Some((depth, scope)) = target else { return };
        let path = &path[depth..];
        let claimed = path.iter().any(|id| {
            self.elements.get(*id).is_some_and(|element| {
                element.flags.unselectable
                    || element.disabled()
                    || !element.listeners(On::Drag).is_empty()
                    || element.kind.text().is_some_and(|text| text.edit.is_some())
            })
        });
        let leaves = self.selection_leaves(scope);
        let hit = (!claimed && self.capture.is_none())
            .then(|| {
                let list = path
                    .iter()
                    .copied()
                    .find(|id| self.row_source(*id).is_some());
                self.selection_hit(
                    leaves
                        .iter()
                        .filter(|leaf| list.is_none() || leaf.list == list),
                    position,
                    path.last().copied(),
                )
            })
            .flatten();
        let Some(hit) = hit else {
            self.clear_selection(scope);
            return;
        };
        let list = hit.leaf.list;
        let anchor = self
            .selection(scope)
            .filter(|state| extend && state.list == list && self.valid_selection(state))
            .and_then(|state| state.range)
            .map(|range| range.anchor);
        let unit = match clicks {
            _ if anchor.is_some() => Unit::Caret,
            0 | 1 => Unit::Caret,
            2 => Unit::Word,
            _ => Unit::Leaf,
        };
        let drag = SelectionDrag {
            scope,
            scroller: list.or_else(|| {
                self.chain(hit.leaf.id)
                    .into_iter()
                    .rev()
                    .find(|id| self.elements[*id].scroll().is_some())
            }),
            start: position,
            position,
            started: false,
            unit,
            origin: anchor.map_or_else(|| hit.span(unit), |anchor| (anchor, anchor)),
            ticked: None,
        };
        self.set_range(scope, list, drag.range(&hit), &leaves);
        self.selection_drag = Some(drag);
    }

    pub(crate) fn move_selection(&mut self, position: Point, slop: f32) {
        let Some(drag) = &mut self.selection_drag else {
            return;
        };
        let travel = position - drag.start;
        drag.started |= travel.x.hypot(travel.y) >= slop;
        drag.position = position;
        let scope = drag.scope;
        if self.selection_overshoot().is_none()
            && let Some(drag) = &mut self.selection_drag
        {
            drag.ticked = None;
        }
        self.refresh_selection(scope, &self.selection_leaves(scope), true);
    }

    pub(crate) fn release_selection(&mut self) -> bool {
        self.selection_drag.take().is_some_and(|drag| drag.started)
    }

    fn selection_overshoot(&self) -> Option<(ElementId, f32)> {
        let drag = self.selection_drag?;
        let scroller = drag.scroller.filter(|id| self.elements.contains_key(*id))?;
        let view = self.window_bounds(scroller);
        let y = drag.position.y;
        let over = y - y.clamp(view.origin.y, view.max().y);
        (over != 0.0).then_some((scroller, over))
    }

    pub(crate) fn selection_wake(&self, now: f64) -> Wake {
        self.selection_overshoot()
            .map_or(Wake::Idle, |_| Wake::At(now + AUTOSCROLL_FRAME))
    }

    pub(crate) fn tick_selection(&mut self, now: f64) {
        let Some((scroller, over)) = self.selection_overshoot() else {
            return;
        };
        let Some(drag) = &mut self.selection_drag else {
            return;
        };
        let Some(last) = drag.ticked.replace(now) else {
            return;
        };
        let scope = drag.scope;
        let speed = (AUTOSCROLL_BASE + over.abs() * AUTOSCROLL_GAIN).min(AUTOSCROLL_MAX);
        let delta = speed.copysign(over) * (now - last).max(0.0) as f32;
        if let Some(layer) = self.layer_id(scroller, Slot::Content) {
            self.scroll_by(layer, Point::new(0.0, delta));
        }
        self.refresh_selection(scope, &self.selection_leaves(scope), true);
    }

    pub(crate) fn refresh_selections(&mut self) {
        for index in 0..self.selection_scopes.len() {
            let scope = self.selection_scopes[index];
            if self
                .selection(scope)
                .is_some_and(|state| state.range.is_some())
            {
                self.refresh_selection(scope, &self.selection_leaves(scope), true);
            }
        }
    }

    fn set_range(
        &mut self,
        scope: ElementId,
        list: Option<ElementId>,
        range: TextRange,
        leaves: &[Leaf],
    ) {
        let keep_endpoints = self
            .selection(scope)
            .is_some_and(|state| state.list == list && self.valid_selection(state));
        if let Some(state) = self.selection_mut(scope) {
            if !keep_endpoints {
                state.endpoints.clear();
            }
            state.list = list;
            state.range = Some(range);
        }
        self.refresh_selection(scope, leaves, false);
    }

    fn select_all(&mut self, scope: ElementId) -> bool {
        let leaves = self.selection_leaves(scope);
        let list = self
            .selection(scope)
            .and_then(|state| state.list)
            .filter(|id| self.chain(*id).contains(&scope))
            .or_else(|| {
                if leaves.iter().any(|leaf| leaf.list.is_none()) {
                    None
                } else {
                    leaves.first().and_then(|leaf| leaf.list)
                }
            });
        let last = match list.and_then(|id| self.row_source(id)) {
            Some(source) => untrack(|| (source.count)()).checked_sub(1),
            None => leaves.iter().any(|leaf| leaf.list.is_none()).then_some(0),
        };
        let Some(last) = last else { return false };
        self.set_range(
            scope,
            list,
            TextRange {
                anchor: TextPoint::default(),
                focus: TextPoint::row_end(last),
            },
            &leaves,
        );
        true
    }

    fn row_source(&self, id: ElementId) -> Option<&RowSource> {
        self.elements.get(id)?.extras()?.row_source.as_ref()
    }

    fn valid_selection(&self, state: &SelectionScope) -> bool {
        let (Some(list), Some(range)) = (state.list, state.range) else {
            return true;
        };
        self.row_source(list).is_some_and(|source| {
            untrack(|| {
                let count = (source.count)();
                range.end().row < count
                    && state.endpoints.iter().all(|endpoint| {
                        endpoint.row < count && endpoint.identity == (source.identity)(endpoint.row)
                    })
            })
        })
    }

    fn refresh_selection(&mut self, scope: ElementId, leaves: &[Leaf], follow_drag: bool) {
        let Some(state) = self.selection(scope) else {
            return;
        };
        let (list, mut range) = (
            state.list,
            state.range.filter(|_| self.valid_selection(state)),
        );
        if let Some(drag) = self.selection_drag.filter(|drag| drag.scope == scope) {
            if range.is_none() {
                self.selection_drag = None;
            } else if follow_drag
                && let Some(hit) = self.selection_hit(
                    leaves.iter().filter(|leaf| leaf.list == list),
                    drag.position,
                    None,
                )
            {
                range = Some(drag.range(&hit));
            }
        }
        let mut endpoints = take(
            &mut self
                .selection_mut(scope)
                .expect("selection scope")
                .endpoints,
        );
        endpoints.retain(|row| range.is_some_and(|range| range.partial_endpoint(row.row)));
        if let Some((source, range)) = list.and_then(|id| self.row_source(id)).zip(range) {
            let start = range.start().row;
            let end = range.end().row;
            let rows = [start, end];
            let unique = if start == end { &rows[..1] } else { &rows[..] };
            for &row in unique.iter().filter(|row| range.partial_endpoint(**row)) {
                let pieces: Vec<_> = leaves
                    .iter()
                    .filter(|leaf| leaf.list == list && leaf.row == row)
                    .map(|leaf| {
                        let (text, separator) = self.leaf_text(leaf);
                        (text.to_owned(), separator)
                    })
                    .collect();
                if !pieces.is_empty() {
                    endpoints.retain(|endpoint| endpoint.row != row);
                    endpoints.push(RowSnapshot {
                        row,
                        identity: untrack(|| (source.identity)(row)),
                        pieces,
                    });
                }
            }
        }
        let state = self.selection_mut(scope).expect("selection scope");
        state.range = range;
        state.endpoints = endpoints;
        let shown = range.filter(|range| range.visible());
        for leaf in leaves {
            let text = self.elements[leaf.id].kind.text_mut().expect("text leaf");
            let selected = shown
                .filter(|_| leaf.list == list)
                .and_then(|range| range.clip(leaf.row, leaf.ordinal, &text.content));
            if text.selected != selected {
                text.selected = selected;
                self.mark_element(leaf.id, Dirty::Paint);
            }
        }
    }

    fn selected_text(&self, scope: ElementId) -> Option<String> {
        let state = self.selection(scope)?;
        let range = state
            .range
            .filter(|range| range.visible() && self.valid_selection(state))?;
        let first = range.start().row;
        let leaves = self.selection_leaves(scope);
        let mut mounted = leaves
            .iter()
            .filter(|leaf| leaf.list == state.list && leaf.row >= first)
            .peekable();
        let row_text = state
            .list
            .and_then(|id| self.row_source(id))
            .and_then(|source| source.text.as_deref());
        let (mut out, mut text) = (String::new(), String::new());
        for row in first..=range.end().row {
            let snapshot = if mounted.peek().is_some_and(|leaf| leaf.row == row) {
                None
            } else {
                state.endpoints.iter().find(|endpoint| endpoint.row == row)
            };
            let current = from_fn(|| mounted.next_if(|leaf| leaf.row == row));
            if let Some(read) = row_text.filter(|_| range.whole_row(row)) {
                current.for_each(drop);
                text = untrack(|| read(row));
            } else {
                let pieces = current.map(|leaf| self.leaf_text(leaf)).chain(
                    snapshot
                        .into_iter()
                        .flat_map(|snapshot| &snapshot.pieces)
                        .map(|(text, separator)| (text.as_str(), *separator)),
                );
                for (leaf, (piece, separator)) in pieces.enumerate() {
                    if let Some(selected) = range.clip(row, leaf, piece) {
                        if !text.is_empty() {
                            text.push_str(separator.as_str());
                        }
                        text.push_str(&piece[selected]);
                    }
                }
            }
            if !text.is_empty() {
                if !out.is_empty() {
                    out.push_str(ROW_BREAK);
                }
                out.push_str(&text);
                text.clear();
            }
        }
        (!out.is_empty()).then_some(out)
    }

    fn leaf_text(&self, leaf: &Leaf) -> (&str, TextBreak) {
        let text = self.elements[leaf.id].kind.text().expect("text leaf");
        (&text.content, text.selection_break)
    }

    fn selection_leaves(&self, scope: ElementId) -> Vec<Leaf> {
        let mut leaves: Vec<Leaf> = Vec::new();
        let mut plain = 0;
        let mut pending = vec![(scope, None, None, true)];
        while let Some((id, list, row, visible)) = pending.pop() {
            let Some(element) = self.elements.get(id) else {
                continue;
            };
            if element.flags.unselectable || (id != scope && self.selection(id).is_some()) {
                continue;
            }
            let list = list.or_else(|| self.row_source(id).map(|_| id));
            let row = row.or_else(|| element.extras().and_then(|extras| extras.select_row));
            let visible = visible && !element.unseen();
            if element.kind.text().is_some_and(|text| text.edit.is_none()) {
                let index = row.unwrap_or(0);
                let ordinal = if list.is_none() {
                    plain += 1;
                    plain - 1
                } else {
                    leaves
                        .last()
                        .filter(|leaf| (leaf.list, leaf.row) == (list, index))
                        .map_or(0, |leaf| leaf.ordinal + 1)
                };
                leaves.push(Leaf {
                    id,
                    list,
                    row: index,
                    ordinal,
                    visible,
                });
            }
            pending.extend(
                element
                    .children
                    .iter()
                    .rev()
                    .map(|child| (*child, list, row, visible)),
            );
        }
        leaves
    }

    fn selection_hit<'a>(
        &self,
        leaves: impl Iterator<Item = &'a Leaf>,
        position: Point,
        target: Option<ElementId>,
    ) -> Option<Hit<'a>> {
        let gap = |value: f32, low: f32, high: f32| (low - value).max(value - high).max(0.0);
        let (_, leaf) = leaves
            .filter(|leaf| leaf.visible)
            .filter_map(|leaf| {
                let text = self.elements[leaf.id].kind.text()?;
                if text.content.is_empty() || text.layout.is_none() {
                    return None;
                }
                let rect = self.window_bounds(leaf.id);
                Some((
                    (
                        target != Some(leaf.id),
                        gap(position.y, rect.origin.y, rect.max().y),
                        gap(position.x, rect.origin.x, rect.max().x),
                    ),
                    leaf,
                ))
            })
            .min_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(Ordering::Equal))?;
        let text = self.elements[leaf.id].kind.text()?;
        let layout = text.layout.as_ref()?;
        let local = self.to_local(leaf.id, position) - text.inset;
        let offset = layout
            .select_at(local.x, local.y, Granularity::Caret)
            .focus();
        let word = layout
            .select_at(local.x, local.y, Granularity::Word)
            .range();
        Some(Hit {
            leaf,
            point: TextPoint::new(leaf.row, leaf.ordinal, offset),
            word,
            len: text.content.len(),
        })
    }
}

impl SelectionDrag {
    fn range(self, hit: &Hit<'_>) -> TextRange {
        let (start, end) = self.origin;
        let (before, after) = hit.span(self.unit);
        let (anchor, focus) = if hit.point < start {
            (end, before)
        } else {
            (start, after)
        };
        TextRange { anchor, focus }
    }
}
