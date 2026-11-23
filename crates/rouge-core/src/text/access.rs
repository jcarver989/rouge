use super::{Shaped, TextLayout, TextSelection};
use accesskit::{
    Node, NodeId, Rect, Role, TextDirection, TextPosition, TextSelection as Selection, Vec2,
};
use parley::{Affinity, Cursor, Selection as ParleySelection};
use rouge_render::Point;
use std::cell::Cell;
use std::mem::{swap, take};
use std::rc::{Rc, Weak};

const MAX_CHARACTERS: usize = u8::MAX as usize + 1;

#[derive(Default, PartialEq)]
struct Characters {
    lengths: Vec<u8>,
    positions: Vec<f32>,
    widths: Vec<f32>,
    words: Vec<u8>,
}

impl Characters {
    fn clear(&mut self) {
        self.lengths.clear();
        self.positions.clear();
        self.widths.clear();
        self.words.clear();
    }
}

struct Span {
    id: NodeId,
    start: usize,
    value: String,
    chars: Characters,
    bounds: Rect,
    direction: TextDirection,
    hard_break: bool,
    line: usize,
    neighbors: [Option<NodeId>; 2],
    dirty: bool,
}

impl Span {
    fn node(&self, origin: Point) -> Node {
        let mut node = Node::new(Role::TextRun);
        node.set_value(self.value.as_str());
        node.set_character_lengths(self.chars.lengths.as_slice());
        node.set_character_positions(self.chars.positions.as_slice());
        node.set_character_widths(self.chars.widths.as_slice());
        node.set_word_starts(self.chars.words.as_slice());
        node.set_text_direction(self.direction);
        node.set_bounds(self.bounds + Vec2::new(origin.x.into(), origin.y.into()));
        if let Some(previous) = self.neighbors[0] {
            node.set_previous_on_line(previous);
        }
        if let Some(next) = self.neighbors[1] {
            node.set_next_on_line(next);
        }
        node
    }
}

#[derive(Default)]
pub(crate) struct TextRuns {
    layout: Weak<Shaped>,
    origin: Point,
    spans: Vec<Span>,
}

impl TextRuns {
    pub(crate) fn matches(&self, layout: &TextLayout, origin: Point) -> bool {
        self.current(layout) && self.origin == origin
    }

    fn current(&self, layout: &TextLayout) -> bool {
        self.layout.as_ptr() == Rc::as_ptr(&layout.shaped)
    }

    pub(crate) fn publish(
        &mut self,
        layout: &TextLayout,
        text: &str,
        origin: Point,
        nodes: &mut Vec<(NodeId, Node)>,
    ) {
        if self.matches(layout, origin) {
            return;
        }
        if !self.current(layout) {
            if self.project(layout, text).is_none() {
                self.spans.clear();
            }
            self.layout = Rc::downgrade(&layout.shaped);
        }
        let moved = self.origin != origin;
        for span in &mut self.spans {
            if take(&mut span.dirty) || moved {
                nodes.push((span.id, span.node(origin)));
            }
        }
        self.origin = origin;
    }

    fn project(&mut self, layout: &TextLayout, text: &str) -> Option<()> {
        let mut index = 0;
        let mut chars = Characters::default();
        let mut visual = Vec::new();
        for (line_index, line) in layout.shaped.layout.lines().enumerate() {
            let metrics = line.metrics();
            let mut x = metrics.offset;
            for run in line.runs() {
                visual.push((run, x));
                x += run.advance();
            }
            visual.sort_unstable_by_key(|(run, _)| run.text_range().start);
            for (run, x) in visual.drain(..) {
                let rtl = run.is_rtl();
                let direction = if rtl {
                    TextDirection::RightToLeft
                } else {
                    TextDirection::LeftToRight
                };
                let mut advance = 0.;
                let mut clusters = run.clusters().peekable();
                loop {
                    chars.clear();
                    let start = clusters.peek().map_or(text.len(), |c| c.text_range().start);
                    let mut end = start;
                    let mut width = 0.;
                    let mut hard_break = false;
                    for cluster in clusters.by_ref().take(MAX_CHARACTERS) {
                        let range = cluster.text_range();
                        if cluster.is_word_boundary()
                            && !text.get(range.clone())?.starts_with(char::is_whitespace)
                        {
                            chars.words.push(chars.lengths.len() as u8);
                        }
                        chars.lengths.push(u8::try_from(range.len()).ok()?);
                        chars.positions.push(width);
                        chars.widths.push(cluster.advance());
                        width += cluster.advance();
                        end = range.end;
                        hard_break = cluster.is_hard_line_break();
                    }
                    let value = text.get(start..end)?;
                    let left = x + if rtl {
                        run.advance() - advance - width
                    } else {
                        advance
                    };
                    let bounds = Rect {
                        x0: left.into(),
                        x1: (left + width).into(),
                        y0: metrics
                            .block_min_coord
                            .min(metrics.content_block_min_coord)
                            .into(),
                        y1: metrics
                            .block_max_coord
                            .max(metrics.content_block_max_coord)
                            .into(),
                    };
                    if let Some(span) = self.spans.get_mut(index) {
                        if span.value != value || span.chars.lengths != chars.lengths {
                            span.id = next_run_id();
                            value.clone_into(&mut span.value);
                            span.dirty = true;
                        }
                        if span.chars != chars {
                            swap(&mut span.chars, &mut chars);
                            span.dirty = true;
                        }
                        span.dirty |= span.bounds != bounds || span.direction != direction;
                        span.direction = direction;
                        span.bounds = bounds;
                        span.start = start;
                        span.line = line_index;
                        span.hard_break = hard_break;
                    } else {
                        self.spans.push(Span {
                            id: next_run_id(),
                            start,
                            value: value.into(),
                            chars: take(&mut chars),
                            bounds,
                            direction,
                            hard_break,
                            line: line_index,
                            neighbors: [None; 2],
                            dirty: true,
                        });
                    }
                    index += 1;
                    advance += width;
                    if clusters.peek().is_none() {
                        break;
                    }
                }
            }
        }
        self.spans.truncate(index);
        link(&mut self.spans);
        Some(())
    }

    pub(crate) fn children(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.spans.iter().map(|span| span.id)
    }

    pub(crate) fn selection(&self, selection: TextSelection) -> Option<Selection> {
        let position = |cursor: Cursor| {
            let index = cursor.index();
            let i = self
                .spans
                .partition_point(|span| span.start <= index)
                .checked_sub(1)?;
            let mut span = &self.spans[i];
            if i > 0 && cursor.affinity() == Affinity::Upstream && span.start == index {
                let previous = &self.spans[i - 1];
                if !previous.hard_break {
                    span = previous;
                }
            }
            let mut byte = span.start;
            for (character_index, &len) in span.chars.lengths.iter().enumerate() {
                if byte == index {
                    return Some(TextPosition {
                        node: span.id,
                        character_index,
                    });
                }
                byte += usize::from(len);
            }
            (byte == index).then_some(TextPosition {
                node: span.id,
                character_index: span.chars.lengths.len(),
            })
        };
        let focus = position(selection.0.focus())?;
        let anchor = if selection.is_collapsed() {
            focus
        } else {
            position(selection.0.anchor())?
        };
        Some(Selection { anchor, focus })
    }

    pub(crate) fn resolve(
        &self,
        layout: &TextLayout,
        selection: Selection,
    ) -> Option<TextSelection> {
        if !self.current(layout) {
            return None;
        }
        let cursor = |position: TextPosition| {
            let span = self.spans.iter().find(|span| span.id == position.node)?;
            let lengths = &span.chars.lengths;
            let bytes = lengths
                .get(..position.character_index)?
                .iter()
                .map(|&n| usize::from(n))
                .sum::<usize>();
            let affinity = if position.character_index == lengths.len() {
                Affinity::Upstream
            } else {
                Affinity::Downstream
            };
            Some(Cursor::from_byte_index(
                &layout.shaped.layout,
                span.start + bytes,
                affinity,
            ))
        };
        Some(TextSelection(ParleySelection::new(
            cursor(selection.anchor)?,
            cursor(selection.focus)?,
        )))
    }
}

fn link(spans: &mut [Span]) {
    for index in 0..spans.len() {
        let line = spans[index].line;
        let on_line =
            |span: Option<&Span>| span.filter(|span| span.line == line).map(|span| span.id);
        let previous = index.checked_sub(1).and_then(|index| spans.get(index));
        let neighbors = [on_line(previous), on_line(spans.get(index + 1))];
        let span = &mut spans[index];
        span.dirty |= span.neighbors != neighbors;
        span.neighbors = neighbors;
    }
}

fn next_run_id() -> NodeId {
    thread_local! { static NEXT: Cell<u64> = const { Cell::new(1) }; }
    let n = NEXT.get();
    NEXT.set(n + 1);
    NodeId(((n >> 32) << 33) | (n & u64::from(u32::MAX)))
}
