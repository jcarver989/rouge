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
use std::mem::take;
use std::ops::Range;
use std::rc::Rc;

const ROW_BREAK: &str = "\n\n";
const AUTOSCROLL_FRAME: f64 = 1.0 / 60.0;
const AUTOSCROLL_BASE: f32 = 240.0;
const AUTOSCROLL_GAIN: f32 = 12.0;
const AUTOSCROLL_MAX: f32 = 6000.0;

#[derive(Clone)]
pub(crate) struct RowSource {
    pub count: Rc<dyn Fn() -> usize>,
    pub identity: Rc<dyn Fn(usize) -> u64>,
    pub text: Option<Rc<dyn Fn(usize) -> String>>,
}

pub(crate) enum SelectOp<'a> {
    Press {
        path: &'a [ElementId],
        position: Point,
        clicks: u32,
        extend: bool,
    },
    Drag(Point),
    Release,
    Edit(&'a [ElementId], EditAction),
    Escape(&'a [ElementId]),
}

#[derive(Default)]
pub(crate) struct SelectionScope {
    range: Option<TextRange>,
    drag: Option<Drag>,
    rows: Vec<RowSnapshot>,
}

impl SelectionScope {
    fn visible(&self) -> Option<TextRange> {
        self.range.filter(|range| range.anchor != range.focus)
    }
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
}

struct RowSnapshot {
    row: usize,
    identity: u64,
    pieces: Vec<Piece>,
}

struct Piece {
    leaf: usize,
    text: String,
    separator: TextBreak,
}

#[derive(Clone, Copy)]
enum Unit {
    Caret,
    Word,
    Leaf,
}

#[derive(Clone, Copy)]
struct Drag {
    position: Point,
    unit: Unit,
    origin: (TextPoint, TextPoint),
    outside: bool,
    ticked: Option<f64>,
}

struct Leaf {
    id: ElementId,
    row: Option<usize>,
    ordinal: usize,
    visible: bool,
}

impl Leaf {
    fn row(&self) -> usize {
        self.row.unwrap_or(0)
    }
}

struct Hit {
    point: TextPoint,
    word: Range<usize>,
    len: usize,
}

impl Hit {
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

    fn selection_scopes(&mut self) -> Vec<ElementId> {
        let mut scopes = take(&mut self.selection_scopes);
        scopes.retain(|id| self.selection(*id).is_some());
        self.selection_scopes.clone_from(&scopes);
        scopes
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

    pub(crate) fn select(&mut self, op: SelectOp<'_>) -> bool {
        match op {
            SelectOp::Press {
                path,
                position,
                clicks,
                extend,
            } => {
                self.press_selection(path, position, clicks, extend);
                false
            }
            SelectOp::Drag(position) => {
                for scope in self.selection_scopes() {
                    let outside = self.outside_scroller(scope, position);
                    let Some(drag) = self
                        .selection_mut(scope)
                        .and_then(|state| state.drag.as_mut())
                    else {
                        continue;
                    };
                    drag.position = position;
                    drag.outside = outside;
                    if !outside {
                        drag.ticked = None;
                    }
                    self.extend_drag(scope);
                }
                false
            }
            SelectOp::Release => {
                for scope in self.selection_scopes() {
                    if let Some(state) = self.selection_mut(scope) {
                        state.drag = None;
                    }
                }
                false
            }
            SelectOp::Edit(path, action) => {
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
            SelectOp::Escape(path) => {
                let Some((_, scope)) = self.scope_in(path) else {
                    return false;
                };
                let visible = self.selection(scope).and_then(SelectionScope::visible);
                self.set_range(scope, None);
                visible.is_some()
            }
        }
    }

    fn press_selection(&mut self, path: &[ElementId], position: Point, clicks: u32, extend: bool) {
        let target = self.scope_in(path);
        for scope in self.selection_scopes() {
            if target.is_none_or(|(_, target)| target != scope) {
                self.set_range(scope, None);
            }
        }
        let Some((depth, scope)) = target else { return };
        let claimed = path[depth + 1..].iter().any(|id| {
            self.elements.get(*id).is_some_and(|element| {
                !element.listeners(On::Drag).is_empty()
                    || element.kind.text().is_some_and(|text| text.edit.is_some())
            })
        });
        let leaves = self.selection_leaves(scope);
        let hit = self.selection_hit(&leaves, position);
        let (Some(hit), false) = (hit, claimed || self.capture.is_some()) else {
            self.set_range(scope, None);
            return;
        };
        let anchor = self
            .selection(scope)
            .and_then(|state| state.range)
            .filter(|_| extend)
            .map(|range| range.anchor);
        let (unit, origin) = match (anchor, clicks) {
            (Some(anchor), _) => (Unit::Caret, (anchor, anchor)),
            (None, 0 | 1) => (Unit::Caret, hit.span(Unit::Caret)),
            (None, 2) => (Unit::Word, hit.span(Unit::Word)),
            (None, _) => (Unit::Leaf, hit.span(Unit::Leaf)),
        };
        let drag = Drag {
            position,
            unit,
            origin,
            outside: false,
            ticked: None,
        };
        if let Some(state) = self.selection_mut(scope) {
            state.drag = Some(drag);
        }
        self.drag_to(scope, drag, &hit);
    }

    pub(crate) fn selection_wake(&self, now: f64) -> Wake {
        let scrolling = self.selection_scopes.iter().any(|id| {
            self.selection(*id)
                .and_then(|state| state.drag)
                .is_some_and(|drag| drag.outside)
        });
        if scrolling {
            Wake::At(now + AUTOSCROLL_FRAME)
        } else {
            Wake::Idle
        }
    }

    pub(crate) fn tick_selection(&mut self, now: f64) {
        for scope in self.selection_scopes() {
            let Some(drag) = self
                .selection_mut(scope)
                .and_then(|state| state.drag.as_mut())
                .filter(|drag| drag.outside)
            else {
                continue;
            };
            let (last, y) = (drag.ticked.replace(now), drag.position.y);
            let (Some(last), Some(scroller)) = (last, self.selection_scroller(scope)) else {
                continue;
            };
            let view = self.window_bounds(scroller);
            let over = if y < view.origin.y {
                y - view.origin.y
            } else {
                y - view.max().y
            };
            let speed = (AUTOSCROLL_BASE + over.abs() * AUTOSCROLL_GAIN).min(AUTOSCROLL_MAX);
            let delta = speed.copysign(over) * (now - last).max(0.0) as f32;
            if let Some(layer) = self.layer_id(scroller, Slot::Content) {
                self.scroll_by(layer, Point::new(0.0, delta));
            }
            self.extend_drag(scope);
        }
    }

    pub(crate) fn refresh_selections(&mut self) {
        for scope in self.selection_scopes() {
            let Some(state) = self.selection(scope).filter(|state| state.range.is_some()) else {
                continue;
            };
            if state.drag.is_some() {
                self.extend_drag(scope);
            }
            self.refresh_selection(scope);
        }
    }

    fn set_range(&mut self, scope: ElementId, range: Option<TextRange>) {
        let Some(state) = self
            .selection_mut(scope)
            .filter(|state| state.range != range)
        else {
            return;
        };
        state.range = range;
        self.refresh_selection(scope);
    }

    fn extend_drag(&mut self, scope: ElementId) {
        let Some(drag) = self.selection(scope).and_then(|state| state.drag) else {
            return;
        };
        let leaves = self.selection_leaves(scope);
        if let Some(hit) = self.selection_hit(&leaves, drag.position) {
            self.drag_to(scope, drag, &hit);
        }
    }

    fn drag_to(&mut self, scope: ElementId, drag: Drag, hit: &Hit) {
        let (start, end) = drag.origin;
        let (before, after) = hit.span(drag.unit);
        let range = if hit.point < start {
            TextRange {
                anchor: end,
                focus: before,
            }
        } else {
            TextRange {
                anchor: start,
                focus: after,
            }
        };
        self.set_range(scope, Some(range));
    }

    fn select_all(&mut self, scope: ElementId) -> bool {
        let last = match self.row_source(scope) {
            Some(source) => untrack(|| (source.count)()).checked_sub(1),
            None => self.selection_leaves(scope).last().map(Leaf::row),
        };
        let Some(last) = last else { return false };
        let range = TextRange {
            anchor: TextPoint::default(),
            focus: TextPoint::new(last, usize::MAX, usize::MAX),
        };
        self.set_range(scope, Some(range));
        true
    }

    fn refresh_selection(&mut self, scope: ElementId) {
        let leaves = self.selection_leaves(scope);
        let source = self.row_source(scope);
        let Some(state) = self.selection_mut(scope) else {
            return;
        };
        let mut range = state.range;
        let mut rows = take(&mut state.rows);
        if let (Some(current), Some(source)) = (range, &source) {
            let outdated = untrack(|| {
                current.start().row >= (source.count)()
                    || rows
                        .iter()
                        .any(|snapshot| snapshot.identity != (source.identity)(snapshot.row))
            });
            if outdated {
                range = None;
            }
        }
        let ends = range.map(|range| [range.start().row, range.end().row]);
        rows.retain(|snapshot| ends.is_some_and(|ends| ends.contains(&snapshot.row)));
        if let Some(ends) = ends {
            for row in ends {
                let pieces = self.pieces(&leaves, row);
                if pieces.is_empty() {
                    continue;
                }
                rows.retain(|snapshot| snapshot.row != row);
                let identity = source
                    .as_ref()
                    .map_or(0, |source| untrack(|| (source.identity)(row)));
                rows.push(RowSnapshot {
                    row,
                    identity,
                    pieces,
                });
            }
        }
        let shown = range.filter(|range| range.anchor != range.focus);
        for leaf in &leaves {
            let Some(text) = self
                .elements
                .get_mut(leaf.id)
                .and_then(|element| element.kind.text_mut())
            else {
                continue;
            };
            let selected = shown.and_then(|range| {
                clip(
                    leaf.row(),
                    leaf.ordinal,
                    &text.content,
                    range.start(),
                    range.end(),
                )
            });
            if text.selected != selected {
                text.selected = selected;
                self.mark_element(leaf.id, Dirty::Paint);
            }
        }
        if let Some(state) = self.selection_mut(scope) {
            state.range = range;
            state.rows = rows;
            if range.is_none() {
                state.drag = None;
            }
        }
    }

    fn selected_text(&self, scope: ElementId) -> Option<String> {
        let state = self.selection(scope)?;
        let range = state.visible()?;
        let (start, end) = (range.start(), range.end());
        let leaves = self.selection_leaves(scope);
        let source = self.row_source(scope);
        let mut out = String::new();
        for row in start.row..=end.row {
            let mounted = self.pieces(&leaves, row);
            let fallback;
            let pieces: &[Piece] = if !mounted.is_empty() {
                &mounted
            } else if let Some(snapshot) = state.rows.iter().find(|snapshot| snapshot.row == row) {
                &snapshot.pieces
            } else if let Some(text) = source.as_ref().and_then(|source| source.text.as_ref()) {
                fallback = [Piece {
                    leaf: 0,
                    text: untrack(|| text(row)),
                    separator: TextBreak::Line,
                }];
                &fallback
            } else {
                continue;
            };
            let mut text = String::new();
            for piece in pieces {
                if let Some(selected) = clip(row, piece.leaf, &piece.text, start, end) {
                    if !text.is_empty() {
                        text.push_str(piece.separator.as_str());
                    }
                    text.push_str(&piece.text[selected]);
                }
            }
            if !text.is_empty() {
                if !out.is_empty() {
                    out.push_str(ROW_BREAK);
                }
                out.push_str(&text);
            }
        }
        (!out.is_empty()).then_some(out)
    }

    fn pieces(&self, leaves: &[Leaf], row: usize) -> Vec<Piece> {
        leaves
            .iter()
            .filter(|leaf| leaf.row() == row)
            .filter_map(|leaf| {
                let text = self.elements.get(leaf.id)?.kind.text()?;
                Some(Piece {
                    leaf: leaf.ordinal,
                    text: text.content.clone(),
                    separator: text.selection_break,
                })
            })
            .collect()
    }

    fn selection_leaves(&self, scope: ElementId) -> Vec<Leaf> {
        let mut leaves = Vec::new();
        let Some(root) = self.elements.get(scope) else {
            return leaves;
        };
        let mut pending: Vec<(ElementId, Option<usize>, bool)> = root
            .children
            .iter()
            .rev()
            .map(|id| (*id, None, !root.unseen()))
            .collect();
        while let Some((id, row, visible)) = pending.pop() {
            let Some(element) = self.elements.get(id) else {
                continue;
            };
            let extras = element.extras();
            if element.flags.unselectable || extras.is_some_and(|extras| extras.selection.is_some())
            {
                continue;
            }
            let row = extras.and_then(|extras| extras.select_row).or(row);
            let visible = visible && !element.unseen();
            if element.kind.text().is_some_and(|text| text.edit.is_none()) {
                leaves.push(Leaf {
                    id,
                    row,
                    ordinal: 0,
                    visible,
                });
            }
            pending.extend(
                element
                    .children
                    .iter()
                    .rev()
                    .map(|child| (*child, row, visible)),
            );
        }
        if leaves.iter().any(|leaf| leaf.row.is_some()) {
            leaves.retain(|leaf| leaf.row.is_some());
        }
        let mut current: Option<(Option<usize>, usize)> = None;
        for leaf in &mut leaves {
            let ordinal = match current {
                Some((row, count)) if row == leaf.row => count,
                _ => 0,
            };
            leaf.ordinal = ordinal;
            current = Some((leaf.row, ordinal + 1));
        }
        leaves
    }

    fn selection_hit(&self, leaves: &[Leaf], position: Point) -> Option<Hit> {
        let gap = |value: f32, low: f32, high: f32| (low - value).max(value - high).max(0.0);
        let (_, leaf) = leaves
            .iter()
            .filter(|leaf| leaf.visible)
            .filter_map(|leaf| {
                let text = self.elements.get(leaf.id)?.kind.text()?;
                if text.content.is_empty() || text.layout.is_none() {
                    return None;
                }
                let rect = self.window_bounds(leaf.id);
                let distance = (
                    gap(position.y, rect.origin.y, rect.max().y),
                    gap(position.x, rect.origin.x, rect.max().x),
                );
                Some((distance, leaf))
            })
            .min_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(Ordering::Equal))?;
        let text = self.elements.get(leaf.id)?.kind.text()?;
        let layout = text.layout.as_ref()?;
        let local = self.to_local(leaf.id, position) - text.inset;
        let offset = layout
            .select_at(local.x, local.y, Granularity::Caret)
            .focus();
        let word = layout
            .select_at(local.x, local.y, Granularity::Word)
            .range();
        Some(Hit {
            point: TextPoint::new(leaf.row(), leaf.ordinal, offset),
            word,
            len: text.content.len(),
        })
    }

    fn row_source(&self, scope: ElementId) -> Option<RowSource> {
        let mut pending = vec![scope];
        while let Some(id) = pending.pop() {
            let element = self.elements.get(id)?;
            if let Some(source) = element
                .extras()
                .and_then(|extras| extras.row_source.as_ref())
            {
                return Some(source.clone());
            }
            pending.extend(element.children.iter().rev());
        }
        None
    }

    fn selection_scroller(&self, scope: ElementId) -> Option<ElementId> {
        let mut pending = vec![scope];
        while let Some(id) = pending.pop() {
            let Some(element) = self.elements.get(id) else {
                continue;
            };
            if element.scroll().is_some() {
                return Some(id);
            }
            pending.extend(element.children.iter().rev());
        }
        self.chain(scope)
            .into_iter()
            .rev()
            .find(|id| self.elements[*id].scroll().is_some())
    }

    fn outside_scroller(&self, scope: ElementId, position: Point) -> bool {
        self.selection_scroller(scope).is_some_and(|scroller| {
            let view = self.window_bounds(scroller);
            position.y < view.origin.y || position.y > view.max().y
        })
    }
}

fn clip(
    row: usize,
    leaf: usize,
    content: &str,
    start: TextPoint,
    end: TextPoint,
) -> Option<Range<usize>> {
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
