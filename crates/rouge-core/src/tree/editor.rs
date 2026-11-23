use super::edit::{CARET_WIDTH, EditState};
use super::{ElementId, TextData, Tree};
use crate::text::{Granularity, Movement, TextLayout, TextRect, boundary};
use rouge_render::{Point, Rect, Size};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ContentType {
    #[default]
    Text,
    Email,
    Url,
    Number,
    Phone,
    Search,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EnterKey {
    #[default]
    Default,
    Done,
    Go,
    Next,
    Search,
    Send,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InputHints {
    pub content: ContentType,
    pub enter: EnterKey,
    pub autocorrect: bool,
}

impl Default for InputHints {
    fn default() -> Self {
        InputHints {
            content: ContentType::default(),
            enter: EnterKey::default(),
            autocorrect: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditTransaction {
    pub(crate) replacements: Vec<(Range<usize>, String)>,
    pub(crate) selection: Option<(usize, usize)>,
    pub(crate) marked: Option<Range<usize>>,
}

impl EditTransaction {
    pub fn new() -> Self {
        EditTransaction::default()
    }

    pub fn between(before: &str, after: &str) -> Self {
        let shared =
            |a: &str, b: &str| a.bytes().zip(b.bytes()).take_while(|(a, b)| a == b).count();
        let start = before.floor_char_boundary(shared(before, after));
        let (old, new) = (&before[start..], &after[start..]);
        let trailing = old
            .bytes()
            .rev()
            .zip(new.bytes().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let end = old.ceil_char_boundary(old.len() - trailing);
        let inserted = &new[..new.len() - (old.len() - end)];
        if end == 0 && inserted.is_empty() {
            return EditTransaction::new();
        }
        EditTransaction::new().replace(start..start + end, inserted)
    }

    pub fn replace(mut self, range: Range<usize>, text: impl Into<String>) -> Self {
        self.replacements.push((range, text.into()));
        self
    }

    pub fn select(mut self, anchor: usize, focus: usize) -> Self {
        self.selection = Some((anchor, focus));
        self
    }

    pub fn mark(mut self, range: Range<usize>) -> Self {
        self.marked = Some(range);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EditorId(ElementId);

pub struct Editor<'a> {
    tree: &'a Tree,
    id: ElementId,
    text: &'a TextData,
    edit: &'a EditState,
    layout: &'a TextLayout,
}

impl<'a> Editor<'a> {
    pub fn id(&self) -> EditorId {
        EditorId(self.id)
    }

    pub fn text(&self) -> &'a str {
        &self.text.content
    }

    pub fn slice(&self, range: Range<usize>) -> &'a str {
        let start = self.clamp(range.start);
        &self.text.content[start..self.clamp(range.end).max(start)]
    }

    pub fn selection(&self) -> (usize, usize) {
        let selection = self.edit.current(self.layout, &self.text.content);
        (selection.anchor(), selection.focus())
    }

    pub fn marked(&self) -> Option<Range<usize>> {
        self.edit.marked(&self.text.content)
    }

    pub fn multiline(&self) -> bool {
        self.edit.multiline
    }

    pub fn hints(&self) -> InputHints {
        self.edit.hints
    }

    pub fn bounds(&self) -> Rect {
        self.tree.window_bounds(self.id)
    }

    pub fn caret(&self, index: usize) -> Rect {
        let index = self.clamp(index);
        let caret = self
            .layout
            .caret(self.layout.select(index, index), CARET_WIDTH);
        self.window(caret)
    }

    pub fn rects(&self, range: Range<usize>) -> Vec<Rect> {
        let (start, end) = (self.clamp(range.start), self.clamp(range.end));
        if start >= end {
            return vec![self.caret(start)];
        }
        self.layout
            .selection_rects(self.layout.select(start, end))
            .into_iter()
            .map(|rect| self.window(rect))
            .collect()
    }

    pub fn index_at(&self, point: Point) -> usize {
        let local = self.tree.to_local(self.id, point) - self.text.offset();
        self.layout
            .select_at(local.x, local.y, Granularity::Caret)
            .focus()
    }

    pub fn moved(&self, index: usize, movement: Movement) -> usize {
        let index = self.clamp(index);
        let from = self.layout.select(index, index);
        self.layout.move_selection(from, movement, false).focus()
    }

    fn clamp(&self, index: usize) -> usize {
        boundary(&self.text.content, index)
    }

    fn window(&self, rect: TextRect) -> Rect {
        self.tree.text_rect(self.id, self.text, rect)
    }
}

impl Tree {
    pub(super) fn text_rect(&self, id: ElementId, text: &TextData, rect: TextRect) -> Rect {
        let origin = self.elements[id].bounds.origin + text.offset() + Point::new(rect.x, rect.y);
        self.to_window(
            id,
            Rect {
                origin,
                size: Size::new(rect.width, rect.height),
            },
        )
    }

    pub(crate) fn editor(&self) -> Option<Editor<'_>> {
        let id = self.focused.filter(|id| self.accepts_value(*id))?;
        let text = self.elements.get(id)?.kind.text()?;
        Some(Editor {
            tree: self,
            id,
            text,
            edit: text.edit.as_deref()?,
            layout: text.layout.as_ref()?,
        })
    }
}
