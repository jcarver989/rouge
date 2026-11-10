use super::{ElementId, TextData, Tree};
use crate::clipboard::Clipboard;
use crate::style::Dirty;
use rouge_reactive::batch;
use rouge_render::{Color, Compositor, Point, Rect, Size};
use rouge_text::{Granularity, Movement, TextLayout, TextSelection, TextStyle, TextSystem};
use std::ops::Range;
use std::rc::Rc;

pub(crate) const CARET_WIDTH: f32 = 1.5;

pub(crate) type TextHandler = Rc<dyn Fn(&str)>;

pub(crate) enum EditOp {
    Insert(String),
    Erase(Movement),
    Move(Movement, bool),
    SelectAll,
    Press {
        local: Point,
        clicks: u32,
        extend: bool,
    },
    Drag {
        local: Point,
    },
    Compose {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    Copy,
    Cut,
    Paste,
}

pub(crate) struct EditState {
    pub multiline: bool,
    pub selection_color: Color,
    pub caret_color: Option<Color>,
    pub on_change: Option<TextHandler>,
    pub(super) selection: TextSelection,
    pub(super) stale: bool,
    pub(super) compose: Option<Range<usize>>,
    pub(super) scroll: f32,
}

impl Default for EditState {
    fn default() -> Self {
        EditState {
            multiline: false,
            selection_color: Color::new(0.2, 0.45, 1.0, 0.35),
            caret_color: None,
            on_change: None,
            selection: TextSelection::default(),
            stale: true,
            compose: None,
            scroll: 0.0,
        }
    }
}

impl EditState {
    pub(super) fn current(&self, layout: &TextLayout, content: &str) -> TextSelection {
        if !self.stale {
            return self.selection;
        }
        let clamp = |index: usize| content.floor_char_boundary(index.min(content.len()));
        layout.select(
            clamp(self.selection.anchor()),
            clamp(self.selection.focus()),
        )
    }

    pub(super) fn compose(&self, layout: &TextLayout, content: &str) -> Option<TextSelection> {
        let range = self
            .compose
            .clone()
            .filter(|range| range.end <= content.len())?;
        Some(layout.select(range.start, range.end))
    }
}

fn ensure<'a>(
    layout: &'a mut Option<TextLayout>,
    content: &str,
    style: &TextStyle,
    width: Option<f32>,
    system: &mut TextSystem,
) -> &'a TextLayout {
    let layout = layout.get_or_insert_with(|| system.layout(content, style));
    layout.break_lines(width);
    layout
}

fn sanitize(text: String, multiline: bool) -> String {
    let keep = |c: &char| !c.is_control() || (multiline && *c == '\n');
    let text = if multiline {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.replace(['\r', '\n'], " ")
    };
    text.chars().filter(keep).collect()
}

fn apply(text: &mut TextData, op: EditOp, system: &mut TextSystem, clipboard: &mut dyn Clipboard) {
    let TextData {
        content,
        style,
        layout,
        edit,
        wrap_width,
        content_size,
        inset,
        ..
    } = text;
    let Some(edit) = edit.as_mut() else { return };
    let current = ensure(layout, content, style, *wrap_width, system);
    let mut selection = edit.current(current, content);
    let point = |local: Point| (local.x - inset.x + edit.scroll, local.y - inset.y);
    let erase = |movement: Movement| {
        if selection.is_collapsed() {
            current.move_selection(selection, movement, true).range()
        } else {
            selection.range()
        }
    };
    let mut replace = None;
    let mut composing = None;
    let mut caret = None;
    match op {
        EditOp::Move(movement, extend) => {
            selection = current.move_selection(selection, movement, extend);
        }
        EditOp::SelectAll => selection = current.select(0, content.len()),
        EditOp::Press {
            local,
            clicks,
            extend,
        } => {
            let (x, y) = point(local);
            selection = match clicks {
                _ if extend => current.extend_selection(selection, x, y, true),
                0 | 1 => current.select_at(x, y, Granularity::Caret),
                2 => current.select_at(x, y, Granularity::Word),
                _ => current.select_at(x, y, Granularity::Line),
            };
        }
        EditOp::Drag { local } => {
            let (x, y) = point(local);
            selection = current.extend_selection(selection, x, y, false);
        }
        EditOp::Insert(text) => {
            let range = edit.compose.take().unwrap_or(selection.range());
            replace = Some((range, sanitize(text, edit.multiline)));
        }
        EditOp::Erase(movement) => replace = Some((erase(movement), String::new())),
        EditOp::Compose { text, cursor } => {
            let range = edit.compose.take().unwrap_or(selection.range());
            composing = (!text.is_empty()).then(|| range.start..range.start + text.len());
            caret =
                cursor.map(|(_, end)| range.start + text.floor_char_boundary(end.min(text.len())));
            replace = Some((range, text));
        }
        EditOp::Copy | EditOp::Cut if selection.is_collapsed() => {}
        EditOp::Copy => clipboard.write(&content[selection.range()]),
        EditOp::Cut => {
            clipboard.write(&content[selection.range()]);
            replace = Some((selection.range(), String::new()));
        }
        EditOp::Paste => {
            let pasted = clipboard.read().map(|text| sanitize(text, edit.multiline));
            replace = pasted
                .filter(|text| !text.is_empty())
                .map(|text| (selection.range(), text));
        }
    }
    if let Some((range, with)) =
        replace.filter(|(range, with)| !(range.is_empty() && with.is_empty()))
    {
        let caret = caret.unwrap_or(range.start + with.len());
        content.replace_range(range, &with);
        *layout = None;
        selection = ensure(layout, content, style, *wrap_width, system).select(caret, caret);
        edit.compose = composing;
    }
    (edit.selection, edit.stale) = (selection, false);
    if !edit.multiline {
        let current = ensure(layout, content, style, *wrap_width, system);
        let caret = current.caret(selection, CARET_WIDTH).x;
        let width = (content_size.width - CARET_WIDTH).max(0.0);
        let limit = (current.size().0 - width).max(0.0);
        edit.scroll = edit.scroll.max(caret - width).min(caret).clamp(0.0, limit);
    }
}

impl Tree {
    pub(crate) fn edit(&mut self, id: ElementId, op: EditOp, system: &mut TextSystem) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let Some(text) = element
            .spec
            .text
            .as_mut()
            .filter(|text| text.edit.is_some())
        else {
            return;
        };
        let before = text.content.clone();
        apply(text, op, system, &mut *self.clipboard);
        let changed = (text.content != before).then(|| text.content.clone());
        let on_change = text.edit.as_ref().and_then(|edit| edit.on_change.clone());
        if changed.is_some() {
            self.taffy.mark_dirty(element.taffy).expect("taffy dirty");
        }
        self.mark_element(
            id,
            if changed.is_some() {
                Dirty::Layout
            } else {
                Dirty::Paint
            },
        );
        if let (Some(content), Some(on_change)) = (changed, on_change) {
            batch(|| on_change(&content));
        }
    }

    pub(crate) fn set_clipboard(&mut self, clipboard: Box<dyn Clipboard>) {
        self.clipboard = clipboard;
    }

    pub(crate) fn caret_area(&self, compositor: &Compositor) -> Option<Rect> {
        let id = self.focused?;
        let element = self.elements.get(id)?;
        let text = element.spec.text.as_ref()?;
        let (edit, layout) = (text.edit.as_ref()?, text.layout.as_ref()?);
        let caret = layout.caret(edit.current(layout, &text.content), CARET_WIDTH);
        let origin =
            element.bounds.origin + text.inset + Point::new(caret.x - edit.scroll, caret.y);
        let rect = Rect {
            origin,
            size: Size::new(caret.width, caret.height),
        };
        Some(self.to_window(compositor, id, rect))
    }
}
