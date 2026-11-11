use super::{ElementId, TextData, Tree};
use crate::clipboard::{Clipboard, ClipboardTarget};
use crate::style::Dirty;
use rouge_reactive::{Signal, batch};
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
    Select(Range<usize>),
    Undo,
    Redo,
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
    PasteText(String),
    CutSelection,
}

#[derive(Clone)]
struct Snapshot {
    content: String,
    anchor: usize,
    focus: usize,
}

#[expect(clippy::struct_excessive_bools)]
pub(crate) struct EditState {
    pub multiline: bool,
    pub min_rows: usize,
    pub max_rows: usize,
    pub selection_signal: Option<Signal<Range<usize>>>,
    pub(super) caret_visible: bool,
    pub(super) activity: f64,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    typing: bool,
    composition: Option<Snapshot>,
    pub selection_color: Color,
    pub caret_color: Option<Color>,
    pub on_change: Option<TextHandler>,
    pub(super) selection: TextSelection,
    pub(super) stale: bool,
    pub(super) compose: Option<Range<usize>>,
    pub(super) scroll: Point,
}

impl Default for EditState {
    fn default() -> Self {
        EditState {
            multiline: false,
            min_rows: 1,
            max_rows: usize::MAX,
            selection_signal: None,
            caret_visible: true,
            activity: 0.0,
            undo: Vec::new(),
            redo: Vec::new(),
            typing: false,
            composition: None,
            selection_color: Color::new(0.2, 0.45, 1.0, 0.35),
            caret_color: None,
            on_change: None,
            selection: TextSelection::default(),
            stale: true,
            compose: None,
            scroll: Point::ZERO,
        }
    }
}

impl EditState {
    pub(super) fn restore(&mut self, previous: EditState, same_content: bool) {
        self.selection = previous.selection;
        self.stale = true;
        self.scroll = previous.scroll;
        if same_content {
            self.undo = previous.undo;
            self.redo = previous.redo;
            self.composition = previous.composition;
            self.compose = previous.compose;
        }
    }

    pub(super) fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.composition = None;
        self.typing = false;
    }

    pub(super) fn scroll_to_caret(&mut self, layout: &TextLayout, content: &str, size: Size) {
        let caret = layout.caret(self.current(layout, content), CARET_WIDTH);
        let (width, height) = layout.size();
        let viewport = (size.width - CARET_WIDTH).max(0.0);
        if self.multiline {
            self.scroll.y = self
                .scroll
                .y
                .max(caret.y + caret.height - size.height)
                .min(caret.y)
                .clamp(
                    0.0,
                    (height.max(caret.y + caret.height) - size.height).max(0.0),
                );
        } else {
            self.scroll.x = self
                .scroll
                .x
                .max(caret.x - viewport)
                .min(caret.x)
                .clamp(0.0, (width - viewport).max(0.0));
        }
    }

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
    let point = |local: Point| {
        (
            local.x - inset.x + edit.scroll.x,
            local.y - inset.y + edit.scroll.y,
        )
    };
    let erase = |movement: Movement| {
        if selection.is_collapsed() {
            current.move_selection(selection, movement, true).range()
        } else {
            selection.range()
        }
    };
    let before = Snapshot {
        content: content.clone(),
        anchor: selection.anchor(),
        focus: selection.focus(),
    };
    let cancel_composition = matches!(&op, EditOp::Compose { text, .. } if text.is_empty())
        && edit.composition.is_some();
    let history = matches!(op, EditOp::Undo | EditOp::Redo) || cancel_composition;
    let typing = matches!(&op, EditOp::Insert(text) if !text.contains('\n') && selection.is_collapsed() && edit.compose.is_none());
    let coalesce = typing && edit.typing;
    let composing_op = matches!(op, EditOp::Compose { .. });
    let compose_commit = matches!(op, EditOp::Insert(_)) && edit.composition.is_some();
    let mut replace = None;
    let mut composing = None;
    let mut caret = None;
    match op {
        EditOp::Undo | EditOp::Redo => {
            let (from, to) = if matches!(op, EditOp::Undo) {
                (&mut edit.undo, &mut edit.redo)
            } else {
                (&mut edit.redo, &mut edit.undo)
            };
            if let Some(snapshot) = from.pop() {
                to.push(before.clone());
                *content = snapshot.content;
                *layout = None;
                selection = ensure(layout, content, style, *wrap_width, system)
                    .select(snapshot.anchor, snapshot.focus);
                edit.compose = None;
                edit.composition = None;
            }
        }
        EditOp::Select(range) => {
            let clamp = |index: usize| content.floor_char_boundary(index.min(content.len()));
            selection = current.select(clamp(range.start), clamp(range.end));
        }
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
        EditOp::Compose { text, cursor: _ } if text.is_empty() && edit.composition.is_some() => {
            if let Some(snapshot) = edit.composition.take() {
                *content = snapshot.content;
                *layout = None;
                selection = ensure(layout, content, style, *wrap_width, system)
                    .select(snapshot.anchor, snapshot.focus);
                edit.compose = None;
            }
        }
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
        EditOp::CutSelection => replace = Some((selection.range(), String::new())),
        EditOp::Paste | EditOp::PasteText(_) => {
            let pasted = match op {
                EditOp::PasteText(text) => Some(text),
                _ => clipboard.read(),
            }
            .map(|text| sanitize(text, edit.multiline));
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
    if (*content != before.content || compose_commit) && !history {
        if composing_op {
            if edit.composition.is_none() {
                edit.composition = Some(before);
            }
        } else {
            if compose_commit {
                if let Some(snapshot) = edit.composition.take() {
                    edit.undo.push(snapshot);
                }
            } else if !coalesce {
                edit.undo.push(before);
            }
            edit.redo.clear();
        }
    }
    edit.typing = typing && !compose_commit;
    (edit.selection, edit.stale) = (selection, false);
    if let Some(signal) = edit.selection_signal.filter(|signal| signal.is_alive()) {
        signal.set_if_changed(selection.range());
    }
    let current = ensure(layout, content, style, *wrap_width, system);
    edit.scroll_to_caret(current, content, *content_size);
}

impl Tree {
    pub(crate) fn caret_animating(&self, now: f64) -> bool {
        self.focused
            .and_then(|id| self.elements.get(id))
            .and_then(|element| element.spec.text.as_ref())
            .and_then(|text| text.edit.as_ref())
            .is_some_and(|edit| now - edit.activity < 10.0)
    }

    pub(crate) fn tick_caret(&mut self, now: f64) {
        self.time = now;
        let Some(id) = self.focused else { return };
        let Some(edit) = self
            .elements
            .get_mut(id)
            .and_then(|element| element.spec.text.as_mut())
            .and_then(|text| text.edit.as_mut())
        else {
            return;
        };
        let elapsed = (now - edit.activity).max(0.0);
        let visible = elapsed >= 10.0 || elapsed % 1.0 < 0.5;
        if edit.caret_visible != visible {
            edit.caret_visible = visible;
            self.mark_element(id, Dirty::Paint);
        }
    }

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
        if let EditOp::Select(range) = &op {
            let layout = ensure(
                &mut text.layout,
                &text.content,
                &text.style,
                text.wrap_width,
                system,
            );
            if text
                .edit
                .as_ref()
                .is_some_and(|edit| edit.current(layout, &text.content).range() == *range)
            {
                return;
            }
        }
        if let Some(edit) = &mut text.edit {
            if self.time - edit.activity > 1.0 {
                edit.typing = false;
            }
            edit.activity = self.time;
            edit.caret_visible = true;
        }
        if self.focused == Some(id) {
            Rc::make_mut(&mut self.clipboard_target);
        }
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

    fn focused_text(&self) -> Option<&TextData> {
        self.elements.get(self.focused?)?.spec.text.as_ref()
    }

    pub(crate) fn is_composing(&self) -> bool {
        self.focused_text()
            .and_then(|text| text.edit.as_ref())
            .is_some_and(|edit| edit.composition.is_some())
    }

    pub(crate) fn clipboard_target(&self) -> Option<ClipboardTarget> {
        self.focused_text()?.edit.as_ref()?;
        (!self.is_composing()).then(|| ClipboardTarget(self.clipboard_target.clone()))
    }

    pub(crate) fn clipboard_selection(
        &mut self,
        system: &mut TextSystem,
    ) -> Option<(ClipboardTarget, String)> {
        let target = self.clipboard_target()?;
        let text = self.elements.get_mut(self.focused?)?.spec.text.as_mut()?;
        let edit = text.edit.as_ref()?;
        let layout = ensure(
            &mut text.layout,
            &text.content,
            &text.style,
            text.wrap_width,
            system,
        );
        let range = edit.current(layout, &text.content).range();
        (!range.is_empty()).then(|| (target, text.content[range].to_owned()))
    }

    pub(crate) fn clipboard_edit(
        &mut self,
        target: &ClipboardTarget,
        op: EditOp,
        system: &mut TextSystem,
    ) -> bool {
        if let Some(id) = self.focused
            && Rc::ptr_eq(&self.clipboard_target, &target.0)
        {
            self.edit(id, op, system);
            true
        } else {
            false
        }
    }

    pub(crate) fn set_clipboard(&mut self, clipboard: Box<dyn Clipboard>) {
        self.clipboard = clipboard;
    }

    pub(crate) fn copy_text(&mut self, text: &str) {
        self.clipboard.write(text);
    }

    pub(crate) fn caret_area(&self, compositor: &Compositor) -> Option<Rect> {
        let id = self.focused?;
        let element = self.elements.get(id)?;
        let text = element.spec.text.as_ref()?;
        let (edit, layout) = (text.edit.as_ref()?, text.layout.as_ref()?);
        let caret = layout.caret(edit.current(layout, &text.content), CARET_WIDTH);
        let origin = element.bounds.origin
            + text.inset
            + Point::new(caret.x - edit.scroll.x, caret.y - edit.scroll.y);
        let rect = Rect {
            origin,
            size: Size::new(caret.width, caret.height),
        };
        Some(self.to_window(compositor, id, rect))
    }
}
