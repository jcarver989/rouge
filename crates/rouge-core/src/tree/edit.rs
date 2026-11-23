use super::content::laid_out;
use super::editor::{EditTransaction, InputHints};
use super::{ElementId, TextData, Tree};
use crate::clipboard::{Clipboard, ClipboardMedia, ClipboardTarget, PasteRequest, Pasted};
use crate::style::Dirty;
use crate::text::{Granularity, Movement, TextLayout, TextSelection, TextSystem, boundary};
use crate::ui::Wake;
use rouge_reactive::{Signal, batch};
use rouge_render::{Color, Point, Rect, Size};
use std::mem::take;
use std::ops::Range;
use std::rc::Rc;

pub(crate) const CARET_WIDTH: f32 = 1.5;
const CARET_HALF_PERIOD: f64 = 0.5;
const CARET_IDLE: f64 = 10.0;

pub(crate) type TextHandler = Rc<dyn Fn(&str)>;
pub(crate) type MediaHandler = Rc<dyn Fn(ClipboardMedia)>;

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
    Transact(EditTransaction),
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
    pub hints: InputHints,
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
    pub on_paste: Option<MediaHandler>,
    pub(super) selection: TextSelection,
    pub(super) stale: bool,
    pub(super) compose: Option<Range<usize>>,
    pub(super) scroll: Point,
}

impl Default for EditState {
    fn default() -> Self {
        EditState {
            multiline: false,
            hints: InputHints::default(),
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
            on_paste: None,
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
        self.compose = None;
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
        layout.select(
            boundary(content, self.selection.anchor()),
            boundary(content, self.selection.focus()),
        )
    }

    pub(super) fn marked(&self, content: &str) -> Option<Range<usize>> {
        self.compose
            .clone()
            .filter(|range| range.end <= content.len())
    }

    pub(super) fn compose(&self, layout: &TextLayout, content: &str) -> Option<TextSelection> {
        let range = self.marked(content)?;
        Some(layout.select(range.start, range.end))
    }
}

impl EditTransaction {
    fn types_at(&self, selection: &TextSelection) -> bool {
        let caret = selection.focus();
        self.marked.is_none()
            && selection.is_collapsed()
            && self.selection.is_none_or(|(anchor, focus)| anchor == focus)
            && matches!(
                self.replacements.as_slice(),
                [(range, with)] if range.is_empty()
                    && range.start == caret
                    && !with.is_empty()
                    && !with.contains('\n')
            )
    }
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
    let current = laid_out(layout, content, style, *wrap_width, system);
    let mut selection = edit.current(current, content);
    let multiline = edit.multiline;
    let point = |local: Point| {
        (
            local.x - inset.x + edit.scroll.x,
            local.y - inset.y + edit.scroll.y,
        )
    };
    let before = Snapshot {
        content: content.clone(),
        anchor: selection.anchor(),
        focus: selection.focus(),
    };
    let target = edit.compose.clone().unwrap_or(selection.range());
    let mut history = false;
    let mut restored = None;
    let mut change = None;
    match op {
        EditOp::Undo | EditOp::Redo => {
            let (from, to) = if matches!(op, EditOp::Undo) {
                (&mut edit.undo, &mut edit.redo)
            } else {
                (&mut edit.redo, &mut edit.undo)
            };
            history = true;
            restored = from.pop();
            if restored.is_some() {
                to.push(before.clone());
                edit.composition = None;
            }
        }
        EditOp::Compose { text, .. } if text.is_empty() => {
            restored = edit.composition.take();
            history = restored.is_some();
        }
        EditOp::Select(range) => {
            selection =
                current.select(boundary(content, range.start), boundary(content, range.end));
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
        EditOp::Copy | EditOp::Cut if selection.is_collapsed() => {}
        EditOp::Copy => clipboard.write(&content[selection.range()]),
        EditOp::Paste => {}
        EditOp::Cut => {
            clipboard.write(&content[selection.range()]);
            change = Some(EditTransaction::new().replace(selection.range(), ""));
        }
        EditOp::Erase(movement) => {
            let range = if selection.is_collapsed() {
                current.move_selection(selection, movement, true).range()
            } else {
                selection.range()
            };
            change = Some(EditTransaction::new().replace(range, ""));
        }
        EditOp::Insert(text) => {
            change = Some(EditTransaction::new().replace(target, sanitize(text, multiline)));
        }
        EditOp::PasteText(text) => {
            change = Some(sanitize(text, multiline))
                .filter(|text| !text.is_empty())
                .map(|text| EditTransaction::new().replace(selection.range(), text));
        }
        EditOp::Compose { text, cursor } => {
            let text = sanitize(text, multiline);
            let end = cursor.map_or(text.len(), |(_, end)| boundary(&text, end));
            let (caret, marked) = (target.start + end, target.start..target.start + text.len());
            change = Some(
                EditTransaction::new()
                    .replace(target, text)
                    .mark(marked)
                    .select(caret, caret),
            );
        }
        EditOp::Transact(mut transaction) => {
            for (_, with) in &mut transaction.replacements {
                *with = sanitize(take(with), multiline);
            }
            change = Some(transaction);
        }
    }
    let typing = edit.compose.is_none()
        && change
            .as_ref()
            .is_some_and(|change| change.types_at(&selection));
    let coalesce = typing && edit.typing;
    let composing = change
        .as_ref()
        .is_some_and(|change| change.marked.is_some());
    let mut commit = false;
    if let Some(snapshot) = restored {
        *content = snapshot.content;
        *layout = None;
        selection = laid_out(layout, content, style, *wrap_width, system)
            .select(snapshot.anchor, snapshot.focus);
        edit.compose = None;
    }
    if let Some(change) = change {
        let mut end = None;
        for (range, with) in change.replacements {
            let start = boundary(content, range.start);
            let range = start..boundary(content, range.end).max(start);
            if range.is_empty() && with.is_empty() {
                continue;
            }
            end = Some(range.start + with.len());
            content.replace_range(range, &with);
            *layout = None;
        }
        if end.is_some() || change.selection.is_some() || change.marked.is_some() {
            let (anchor, focus) = change
                .selection
                .or(end.map(|end| (end, end)))
                .unwrap_or((selection.anchor(), selection.focus()));
            selection = laid_out(layout, content, style, *wrap_width, system)
                .select(boundary(content, anchor), boundary(content, focus));
            edit.compose = change
                .marked
                .map(|range| boundary(content, range.start)..boundary(content, range.end))
                .filter(|range| range.start < range.end);
            commit = !composing && edit.composition.is_some();
        }
    }
    if (*content != before.content || commit) && !history {
        if composing {
            if edit.composition.is_none() {
                edit.composition = Some(before);
            }
        } else {
            if commit {
                if let Some(snapshot) = edit.composition.take()
                    && snapshot.content != *content
                {
                    edit.undo.push(snapshot);
                }
            } else if !coalesce {
                edit.undo.push(before);
            }
            edit.redo.clear();
        }
    }
    edit.typing = typing && !commit;
    (edit.selection, edit.stale) = (selection, false);
    if let Some(signal) = edit.selection_signal.filter(|signal| signal.is_alive()) {
        signal.set_if_changed(selection.range());
    }
    let current = laid_out(layout, content, style, *wrap_width, system);
    edit.scroll_to_caret(current, content, *content_size);
}

impl Tree {
    pub(crate) fn caret_wake(&self, now: f64) -> Wake {
        let Some(edit) = self
            .focused
            .and_then(|id| self.elements.get(id))
            .and_then(|element| element.kind.text())
            .and_then(|text| text.edit.as_ref())
        else {
            return Wake::Idle;
        };
        let elapsed = (now - edit.activity).max(0.0);
        if caret_visible(elapsed) != edit.caret_visible {
            return Wake::Now;
        }
        if elapsed >= CARET_IDLE {
            return Wake::Idle;
        }
        let next = (caret_phase(elapsed) + 1) as f64 * CARET_HALF_PERIOD;
        Wake::At(edit.activity + next.min(CARET_IDLE))
    }

    pub(crate) fn tick_caret(&mut self, now: f64) {
        self.time = now;
        let Some(id) = self.focused else { return };
        let Some(edit) = self
            .elements
            .get_mut(id)
            .and_then(|element| element.kind.text_mut())
            .and_then(|text| text.edit.as_mut())
        else {
            return;
        };
        let visible = caret_visible((now - edit.activity).max(0.0));
        if edit.caret_visible != visible {
            edit.caret_visible = visible;
            self.mark_element(id, Dirty::Paint);
        }
    }

    pub(crate) fn edit(&mut self, id: ElementId, op: EditOp, system: &mut TextSystem) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let Some(text) = element.kind.text_mut().filter(|text| text.edit.is_some()) else {
            return;
        };
        if let EditOp::Paste = op {
            let media = text
                .edit
                .as_ref()
                .is_some_and(|edit| edit.on_paste.is_some());
            let request = PasteRequest {
                element: id,
                target: ClipboardTarget(self.clipboard_target.clone()),
                media,
                queue: self.queue.clone(),
            };
            self.clipboard.read(request);
            return;
        }
        if let EditOp::Select(range) = &op {
            let layout = laid_out(
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
            self.invalidate_layout(id);
            self.mark_access(id);
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
        self.elements.get(self.focused?)?.kind.text()
    }

    pub(crate) fn is_composing(&self) -> bool {
        self.focused_text()
            .and_then(|text| text.edit.as_ref())
            .is_some_and(|edit| edit.composition.is_some())
    }

    pub(crate) fn paste(
        &mut self,
        id: ElementId,
        target: &ClipboardTarget,
        pasted: Pasted,
        system: &mut TextSystem,
    ) {
        if self.focused != Some(id)
            || self.is_composing()
            || !Rc::ptr_eq(&self.clipboard_target, &target.0)
        {
            return;
        }
        match pasted {
            Pasted::Text(text) => self.edit(id, EditOp::PasteText(text), system),
            Pasted::Media(media) => {
                let on_paste = self
                    .focused_text()
                    .and_then(|text| text.edit.as_ref())
                    .and_then(|edit| edit.on_paste.clone());
                if let Some(on_paste) = on_paste {
                    batch(|| on_paste(media));
                }
            }
        }
    }

    pub(crate) fn set_clipboard(&mut self, clipboard: Box<dyn Clipboard>) {
        self.clipboard = clipboard;
    }

    pub(crate) fn copy_text(&mut self, text: &str) {
        self.clipboard.write(text);
    }

    pub(crate) fn caret_area(&self) -> Option<Rect> {
        let id = self.focused?;
        let text = self.elements.get(id)?.kind.text()?;
        let (edit, layout) = (text.edit.as_ref()?, text.layout.as_ref()?);
        let caret = layout.caret(edit.current(layout, &text.content), CARET_WIDTH);
        Some(self.text_rect(id, text, caret))
    }
}

fn caret_phase(elapsed: f64) -> u64 {
    (elapsed / CARET_HALF_PERIOD + 1.0e-6).floor() as u64
}

fn caret_visible(elapsed: f64) -> bool {
    elapsed + 1.0e-6 * CARET_HALF_PERIOD >= CARET_IDLE || caret_phase(elapsed).is_multiple_of(2)
}
