use super::layers::Slot;
use super::{Element, ElementId, Interaction, Tree};
use crate::event::{
    Chord, Cursor, DragEvent, FileDropEvent, FocusEvent, ImeEvent, KeyEvent, Modifiers,
    PointerEvent, ScrollEvent,
};
use crate::keymap::Keymap;
use rouge_reactive::batch;
use rouge_render::Point;
use std::any::{Any, TypeId};
use std::mem::{replace, take};
use std::rc::Rc;

pub(crate) type ActionHandler = Rc<dyn Fn(&dyn Any)>;
pub(crate) type Listener = Rc<dyn Fn(&Event<'_>)>;
pub(crate) type Listeners = Vec<(ElementId, Vec<Listener>)>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum On {
    Click,
    PointerDown,
    PointerUp,
    PointerMove,
    KeyDown,
    KeyUp,
    Focus,
    Blur,
    Hover,
    Ime,
    Drag,
    Scroll,
    FileDrop,
}

#[derive(Clone, Copy)]
pub(crate) enum Event<'a> {
    Pointer(&'a PointerEvent),
    Key(&'a KeyEvent),
    Focus(&'a FocusEvent),
    Hover(bool),
    Ime(&'a ImeEvent),
    Drag(&'a DragEvent),
    Scroll(&'a ScrollEvent),
    FileDrop(&'a FileDropEvent),
}

impl Event<'_> {
    fn stopped(&self) -> bool {
        match self {
            Event::Pointer(event) => event.stopped.get(),
            Event::Key(event) => event.stopped.get(),
            Event::Scroll(event) => event.stopped.get(),
            Event::FileDrop(event) => event.stopped.get(),
            _ => false,
        }
    }
}

pub(crate) fn emit(listeners: &Listeners, event: Event<'_>, mut each: impl FnMut(ElementId)) {
    batch(|| {
        for (element, listeners) in listeners {
            each(*element);
            for listener in listeners {
                listener(&event);
            }
            if event.stopped() {
                break;
            }
        }
    });
}

impl Element {
    pub(super) fn listeners(&self, on: On) -> Vec<Listener> {
        let all = self.extras().map_or(&[][..], |extras| &extras.listeners);
        all.iter()
            .filter(|(kind, _)| *kind == on)
            .map(|(_, listener)| listener.clone())
            .collect()
    }
}

impl Tree {
    pub(crate) fn listeners(&self, path: &[ElementId], on: On) -> Listeners {
        let pointer = matches!(
            on,
            On::Click | On::PointerDown | On::PointerUp | On::PointerMove
        );
        let disabled = |id: &ElementId| self.elements.get(*id).is_some_and(Element::disabled);
        let enabled = path.iter().position(disabled).filter(|_| pointer);
        let found = path[..enabled.unwrap_or(path.len())].iter().rev();
        found
            .filter_map(|id| Some((*id, self.elements.get(*id)?.listeners(on))))
            .filter(|(_, listeners)| !listeners.is_empty())
            .take(if on == On::Ime { 1 } else { usize::MAX })
            .collect()
    }

    pub(crate) fn observe_keymap(&mut self, keymap: Keymap) {
        self.keymap = Some(keymap);
    }

    pub(crate) fn publish_bindings(&self) {
        if let Some(keymap) = self.keymap {
            keymap.0.set_if_changed(self.effective_bindings());
        }
    }

    pub(crate) fn effective_bindings(&self) -> Vec<(TypeId, Chord)> {
        let path = self.focus_path();
        let mut bindings = Vec::new();
        let mut seen: Vec<Chord> = Vec::new();
        let extras = path
            .iter()
            .rev()
            .filter_map(|id| self.elements.get(*id)?.extras());
        for extras in extras {
            for (chord, action) in &extras.bindings {
                let event = KeyEvent::new(chord.key.clone()).with_modifiers(chord.modifiers);
                if seen.iter().any(|chord| chord.matches(&event)) {
                    continue;
                }
                seen.push(chord.clone());
                let kind = (**action).type_id();
                if !self.action_handlers(&path, kind).is_empty() {
                    bindings.push((kind, chord.clone()));
                }
            }
        }
        bindings
    }

    pub(crate) fn action_handlers(&self, path: &[ElementId], kind: TypeId) -> Vec<ActionHandler> {
        path.iter()
            .rev()
            .filter_map(|id| self.elements.get(*id)?.extras())
            .find_map(|extras| {
                let handlers: Vec<_> = extras
                    .actions
                    .iter()
                    .filter(|(id, _)| *id == kind)
                    .map(|(_, handler)| handler.clone())
                    .collect();
                (!handlers.is_empty()).then_some(handlers)
            })
            .unwrap_or_default()
    }

    pub(crate) fn binding(
        &self,
        path: &[ElementId],
        event: &KeyEvent,
    ) -> Option<(Rc<dyn Any>, Vec<ActionHandler>)> {
        let mut extras = path
            .iter()
            .rev()
            .filter_map(|id| self.elements.get(*id)?.extras());
        let action = extras.find_map(|extras| {
            let bound = extras
                .bindings
                .iter()
                .find(|(chord, _)| chord.matches(event));
            bound.map(|(_, action)| action.clone())
        })?;
        let handlers = self.action_handlers(path, (*action).type_id());
        if handlers.is_empty() {
            return None;
        }
        Some((action, handlers))
    }

    pub(crate) fn cursor(&self) -> Cursor {
        let path = self.pointer_path();
        if let Some(cursor) = self.selection_cursor(path) {
            return cursor;
        }
        let mut path = path.iter().rev();
        path.find_map(|id| {
            let element = self.elements.get(*id)?;
            if element.disabled() {
                return Some(Cursor::default());
            }
            element.cursor
        })
        .unwrap_or_default()
    }

    pub(crate) fn text_click(&self, path: &[ElementId], point: Point, modifiers: Modifiers) {
        batch(|| {
            for id in path.iter().rev() {
                let extras = self.elements[*id].extras();
                if let Some(callback) = extras.and_then(|extras| extras.text_click.as_ref()) {
                    callback(self.text_hit(*id, point), modifiers);
                }
            }
        });
    }

    pub(crate) fn hit_test(&self, point: Point) -> Vec<ElementId> {
        let mut path = Vec::new();
        self.hit_test_in(self.root, point, &mut path);
        path
    }

    fn hit_test_in(&self, id: ElementId, point: Point, path: &mut Vec<ElementId>) -> bool {
        let element = &self.elements[id];
        if element.unseen() {
            return false;
        }
        let through = |slot: Slot, point: Point| {
            let layer = element.layer(slot);
            let Some(pose) = layer.and_then(|layer| self.layers.pose(layer.id)) else {
                return Some(point);
            };
            let inverse = pose
                .transform(element.bounds)
                .inverse()
                .filter(|_| pose.opacity > 0.0)?;
            Some(inverse.apply(point))
        };
        let Some(point) = through(Slot::Own, point) else {
            return false;
        };
        let inside = element.bounds.contains(point);
        if !inside && element.clips_children() {
            return false;
        }
        path.push(id);
        let inner = through(Slot::Content, point);
        let hit_child = inner.is_some_and(|point| {
            element
                .children
                .iter()
                .rev()
                .any(|child| self.hit_test_in(*child, point, path))
        });
        if hit_child || (inside && !element.flags.pass_through) {
            return true;
        }
        path.pop();
        false
    }

    pub(crate) fn set_pointer(&mut self, point: Option<Point>, modifiers: Modifiers) {
        if self.capture.is_some() {
            return;
        }
        let path = point.map_or_default(|p| self.hit_test(p));
        let text_hover = |id: &ElementId| {
            let extras = self.elements[*id].extras();
            extras.and_then(|extras| extras.text_hover.as_ref())
        };
        batch(|| {
            for id in self.hover_path.iter().filter(|id| !path.contains(id)) {
                if let Some(callback) = text_hover(id) {
                    callback(None, modifiers);
                }
            }
            if let Some(point) = point {
                for id in &path {
                    if let Some(callback) = text_hover(id) {
                        callback(self.text_hit(*id, point), modifiers);
                    }
                }
            }
        });
        if path == self.hover_path {
            return;
        }
        let old = replace(&mut self.hover_path, path.clone());
        let mut calls = Vec::new();
        for (from, to, on) in [(&old, &path, false), (&path, &old, true)] {
            for id in from.iter().filter(|id| !to.contains(id)) {
                self.set_interaction(*id, Interaction::Hover, on);
                calls.push((self.listeners(&[*id], On::Hover), on));
            }
        }
        batch(|| {
            for (listeners, on) in &calls {
                emit(listeners, Event::Hover(*on), |_| {});
            }
        });
    }

    pub(crate) fn press(&mut self) {
        self.press_path = self.hover_path.clone();
        for id in self.press_path.clone() {
            self.set_interaction(id, Interaction::Active, true);
        }
    }

    pub(crate) fn release(&mut self) -> Vec<ElementId> {
        let mut pressed = take(&mut self.press_path);
        for id in &pressed {
            self.set_interaction(*id, Interaction::Active, false);
        }
        pressed.retain(|id| self.hover_path.contains(id));
        pressed
    }

    pub(crate) fn hover_path(&self) -> &[ElementId] {
        &self.hover_path
    }

    pub(crate) fn capture(&mut self, path: &[ElementId], id: ElementId) {
        let depth = path.iter().position(|element| *element == id);
        self.capture = depth.map(|depth| path[..=depth].to_vec());
    }

    pub(crate) fn release_capture(&mut self) {
        self.capture = None;
    }

    pub(crate) fn pointer_path(&self) -> &[ElementId] {
        self.capture.as_deref().unwrap_or(&self.hover_path)
    }
}
