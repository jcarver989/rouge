use super::edit::EditState;
use super::{ElementId, Kind, Tree};
use crate::canvas::{PaintOp, ShapedRun};
use crate::selection::TextBreak;
use crate::style::Dirty;
use crate::text::{TextLayout, TextStyle, TextSystem};
use crate::theme::Fill;
use crate::view::TextOp;
use rouge_render::{Color, ImageData, Point, Size};
use rouge_theme::Role;
use std::ops::Range;
use std::rc::Rc;

pub(crate) struct TextData {
    pub content: String,
    pub style: TextStyle,
    pub color: Fill,
    pub edit: Option<Box<EditState>>,
    pub placeholder: String,
    pub placeholder_color: Fill,
    pub selection_break: TextBreak,
    pub(super) selected: Option<Range<usize>>,
    pub(super) layout: Option<TextLayout>,
    pub(super) inset: Point,
    pub(super) wrap_width: Option<f32>,
    pub(super) content_size: Size,
}

impl Default for TextData {
    fn default() -> Self {
        TextData {
            content: String::new(),
            style: TextStyle::default(),
            color: Fill::Role(Role::Text),
            edit: None,
            placeholder: String::new(),
            placeholder_color: Fill::Color(Color::TRANSPARENT),
            selection_break: TextBreak::default(),
            selected: None,
            layout: None,
            inset: Point::ZERO,
            wrap_width: None,
            content_size: Size::default(),
        }
    }
}

impl TextData {
    pub(super) fn shaped(&mut self, system: &mut TextSystem) -> &TextLayout {
        self.layout
            .get_or_insert_with(|| system.layout(&self.content, &self.style, self.wrap_width))
    }

    pub(super) fn laid_out(&mut self, system: &mut TextSystem, width: Option<f32>) -> &TextLayout {
        laid_out(&mut self.layout, &self.content, &self.style, width, system)
    }
}

impl Tree {
    pub(crate) fn update_text(
        &mut self,
        id: ElementId,
        dirty: Dirty,
        f: impl FnOnce(&mut TextData),
    ) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let Some(text) = element.kind.text_mut() else {
            return;
        };
        let previous = text.content.clone();
        f(text);
        if previous != text.content
            && let Some(edit) = &mut text.edit
        {
            edit.clear_history();
            if self.focused == Some(id) {
                Rc::make_mut(&mut self.clipboard_target);
            }
        }
        if dirty.needs_layout() {
            text.layout = None;
            if let Some(edit) = &mut text.edit {
                edit.stale = true;
            }
            self.invalidate_layout(id);
        }
        self.mark_element(id, dirty);
        self.mark_access(id);
    }

    pub(crate) fn update_text_prop(&mut self, id: ElementId, op: TextOp) {
        match op {
            TextOp::Spans(spans) => {
                self.update_text(id, Dirty::Layout, |text| text.style.spans = spans);
            }
            TextOp::Placeholder(placeholder) => {
                self.update_text(id, Dirty::Paint, |text| text.placeholder = placeholder);
            }
            TextOp::PlaceholderColor(color) => {
                self.update_text(id, Dirty::Paint, |text| text.placeholder_color = color);
            }
        }
    }

    pub(crate) fn set_text_content(&mut self, id: ElementId, content: String) {
        let text = self.elements.get(id).and_then(|e| e.kind.text());
        if text.is_some_and(|text| text.content != content) {
            self.update_text(id, Dirty::Layout, |text| text.content = content);
        }
    }

    pub(crate) fn set_image(&mut self, id: ElementId, data: Option<ImageData>) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let Kind::Image(image) = &mut element.kind else {
            return;
        };
        let resized = image
            .data
            .as_ref()
            .map(|data| (data.width(), data.height()))
            != data.as_ref().map(|data| (data.width(), data.height()));
        image.data = data;
        if resized {
            self.invalidate_layout(id);
        }
        self.mark_element(id, if resized { Dirty::Layout } else { Dirty::Paint });
    }

    pub(crate) fn set_canvas(
        &mut self,
        id: ElementId,
        mut ops: Vec<PaintOp>,
        system: &mut TextSystem,
    ) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let Kind::Canvas(canvas) = &mut element.kind else {
            return;
        };
        let mut overflow = 0.0_f32;
        let mut marks = Vec::new();
        for op in &mut ops {
            if let PaintOp::Text(shaped) = op {
                shape_run(shaped, system);
                overflow = overflow.max(shaped.overflow());
                marks.extend(shaped.continuation_marks());
            }
        }
        ops.extend(marks.into_iter().map(|run| {
            let mut shaped = ShapedRun { run, layout: None };
            shape_run(&mut shaped, system);
            PaintOp::Text(shaped)
        }));
        if let Some(observer) = canvas.overflow.filter(|o| o.is_alive()) {
            observer.set_if_changed(overflow);
        }
        let tracks = ops
            .iter()
            .any(|op| matches!(op, PaintOp::Text(shaped) if shaped.run.track.is_some()));
        canvas.ops = ops;
        if tracks && !self.observers.contains(&id) {
            self.observers.push(id);
        }
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let Kind::Canvas(canvas) = &mut element.kind else {
            return;
        };
        let laid_out = element.bounds.size.width;
        let resized = canvas.measure.as_mut().is_some_and(|measure| {
            let Some(last) = measure
                .last
                .or((laid_out > 0.0).then(|| Size::new(laid_out, f32::NAN)))
            else {
                return false;
            };
            let height = measure.height(system, last.width);
            measure.last = Some(Size::new(last.width, height));
            height != last.height
        });
        if resized {
            self.invalidate_layout(id);
        }
        self.mark_element(id, if resized { Dirty::Layout } else { Dirty::Paint });
    }
}

pub(super) fn laid_out<'a>(
    layout: &'a mut Option<TextLayout>,
    content: &str,
    style: &TextStyle,
    width: Option<f32>,
    system: &mut TextSystem,
) -> &'a TextLayout {
    if layout.as_ref().is_some_and(|layout| !layout.fits(width)) {
        *layout = None;
    }
    layout.get_or_insert_with(|| system.layout(content, style, width))
}

fn shape_run(shaped: &mut ShapedRun, system: &mut TextSystem) {
    let layout = system.layout(&shaped.run.content, &shaped.run.style, shaped.run.wrap);
    if shaped.run.right_aligned {
        shaped.run.origin.x -= layout.size().0;
        shaped.run.right_aligned = false;
    }
    shaped.layout = Some(layout);
}
