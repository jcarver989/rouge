use super::edit::EditState;
use super::events::{Event, Listener, On};
use super::{Element, ElementId, Interaction, Tree};
use crate::event::FocusEvent;
use crate::style::Dirty;
use rouge_reactive::{Signal, batch};
use std::rc::Rc;

pub(super) type ReplacedFocus = (u64, Option<(String, Box<EditState>)>);

#[derive(Clone, Copy, Default)]
pub(crate) struct FocusSpec {
    pub tab_index: i32,
    pub handle: Option<(u64, Signal<bool>)>,
}

impl Element {
    fn focus(&self) -> Option<FocusSpec> {
        self.extras()?.focus
    }
}

impl Tree {
    pub(crate) fn focused(&self) -> Option<ElementId> {
        self.focused
    }

    pub(crate) fn settle_focus(&mut self) {
        let fallback = self.focus_fallback.take();
        self.restore_replaced_focus();
        if self.focused.is_none()
            && let Some(fallback) = fallback.filter(|id| self.elements.contains_key(*id))
        {
            self.set_focus(Some(fallback), self.focus_visible);
        }
    }

    pub(crate) fn focus_path(&self) -> Vec<ElementId> {
        if let Some(focused) = self.focused {
            return self.chain(focused);
        }
        let mut path = vec![self.root];
        let first = self.elements[self.root].children.first().copied();
        let mut view = first.filter(|id| Some(*id) != self.overlay);
        while let Some(id) = view.take() {
            path.push(id);
            let mut children = self.elements[id].children.iter().copied();
            if let Some(host) = children.find(|id| self.elements[*id].flags.root_host) {
                path.push(host);
                view = self.elements[host].children.first().copied();
            }
        }
        path
    }

    pub(crate) fn set_focus(&mut self, target: Option<ElementId>, visible: bool) {
        let target = target.filter(|id| self.is_focusable(*id));
        let previous = self.focused;
        if previous == target && (self.focus_visible == visible || target.is_none()) {
            return;
        }
        (self.focused, self.focus_visible) = (target, visible);
        let mut calls: Vec<(Vec<Listener>, Option<Signal<bool>>, bool)> = Vec::new();
        if previous != target {
            Rc::make_mut(&mut self.clipboard_target);
            for (id, on) in [(previous, false), (target, true)] {
                let Some(element) = id.and_then(|id| self.elements.get(id)) else {
                    continue;
                };
                let handlers = element.listeners(if on { On::Focus } else { On::Blur });
                let handle = element
                    .focus()
                    .and_then(|focus| focus.handle)
                    .map(|(_, focused)| focused);
                let editable = element.kind.text().is_some_and(|text| text.edit.is_some());
                calls.push((handlers, handle, on));
                if editable {
                    self.mark_element(id.unwrap_or_default(), Dirty::Paint);
                }
            }
        }
        for id in previous.into_iter().filter(|id| Some(*id) != target) {
            self.set_interaction(id, Interaction::Focus, false);
            self.set_interaction(id, Interaction::FocusVisible, false);
        }
        if let Some(id) = target {
            let edit = self.elements[id].kind.text_mut();
            if let Some(edit) = edit.and_then(|text| text.edit.as_mut()) {
                edit.activity = self.time;
                edit.caret_visible = true;
            }
            self.set_interaction(id, Interaction::Focus, true);
            self.set_interaction(id, Interaction::FocusVisible, visible);
        }
        self.publish_bindings();
        let event = FocusEvent { visible };
        batch(|| {
            for (handlers, handle, on) in &calls {
                if let Some(focused) = handle.filter(|focused| focused.is_alive()) {
                    focused.set_if_changed(*on);
                }
                for handler in handlers {
                    handler(&Event::Focus(&event));
                }
            }
        });
    }

    pub(crate) fn focus_pointer(&mut self) {
        if self
            .hover_path
            .iter()
            .any(|id| self.elements[*id].flags.preserve_focus)
        {
            return;
        }
        let target = self
            .hover_path
            .iter()
            .rev()
            .find(|id| self.is_focusable(**id))
            .copied();
        self.set_focus(target, false);
    }

    pub(crate) fn focus_handle(&mut self, handle: u64, focus: bool) {
        let Some((element, _)) = self.handles.get(&handle).copied() else {
            return;
        };
        if focus {
            self.set_focus(Some(element), self.focus_visible);
        } else if self.focused == Some(element) {
            self.set_focus(None, false);
        }
    }

    pub(crate) fn autofocus(&mut self, id: ElementId) {
        if self.is_focusable(id) && self.focused != Some(id) {
            self.restore.push((id, self.focused));
            self.set_focus(Some(id), self.focus_visible);
        }
    }

    pub(crate) fn focus_next(&mut self, reverse: bool) -> bool {
        let stops = self.tab_stops();
        if stops.is_empty() {
            return false;
        }
        let current = self
            .focused
            .and_then(|focused| stops.iter().position(|stop| *stop == focused));
        let next = match (current, reverse) {
            (Some(index), false) => (index + 1) % stops.len(),
            (Some(index), true) => (index + stops.len() - 1) % stops.len(),
            (None, false) => 0,
            (None, true) => stops.len() - 1,
        };
        self.set_focus(Some(stops[next]), true);
        true
    }

    pub(crate) fn set_disabled(&mut self, id: ElementId, disabled: bool) {
        self.set_interaction(id, Interaction::Disabled, disabled);
        if self
            .focused
            .is_some_and(|focused| self.is_disabled(focused))
        {
            self.set_focus(None, false);
        }
    }

    pub(super) fn is_disabled(&self, id: ElementId) -> bool {
        self.chain(id)
            .iter()
            .any(|id| self.elements[*id].disabled())
    }

    pub(super) fn replace_focus(&mut self, removed: &[ElementId]) {
        let Some(focused) = self
            .focused
            .filter(|id| self.chain(*id).iter().any(|id| removed.contains(id)))
        else {
            return;
        };
        let element = &mut self.elements[focused];
        if let Some((handle, _)) = element.focus().and_then(|focus| focus.handle) {
            let state = element
                .kind
                .text_mut()
                .and_then(|text| text.edit.take().map(|edit| (text.content.clone(), edit)));
            self.replaced_focus = Some((handle, state));
        }
    }

    pub(super) fn forget_focus(&mut self) {
        let elements = &self.elements;
        self.handles.retain(|_, (element, focused)| {
            let alive = elements.contains_key(*element);
            if !alive && focused.is_alive() {
                focused.set_if_changed(false);
            }
            alive
        });
        let alive = |id: &ElementId| elements.contains_key(*id);
        let mut gone = self.restore.iter().rev().filter(|(owner, _)| !alive(owner));
        let fallback = gone.find_map(|(_, previous)| previous.filter(alive));
        self.restore.retain(|(owner, _)| alive(owner));
        if self.focused.is_some_and(|focused| !alive(&focused)) {
            self.focused = None;
            Rc::make_mut(&mut self.clipboard_target);
            self.focus_fallback = fallback;
        } else if self.focus_fallback.is_some_and(|pending| !alive(&pending)) {
            self.focus_fallback = fallback;
        }
    }

    fn restore_replaced_focus(&mut self) {
        let Some((handle, edit)) = self.replaced_focus.take() else {
            return;
        };
        let Some((new, _)) = self
            .handles
            .get(&handle)
            .copied()
            .filter(|(id, _)| self.is_focusable(*id))
        else {
            return;
        };
        let text = self
            .elements
            .get_mut(new)
            .and_then(|element| element.kind.text_mut());
        if let (Some((content, old)), Some(text)) = (edit, text)
            && let Some(edit) = &mut text.edit
        {
            edit.restore(*old, content == text.content);
        }
        self.set_focus(Some(new), self.focus_visible);
    }

    fn is_focusable(&self, id: ElementId) -> bool {
        let chain = self.chain(id);
        let attached = chain.first() == Some(&self.root);
        let usable = chain.iter().all(|id| {
            let element = &self.elements[*id];
            !element.unseen() && !element.disabled()
        });
        attached && usable && self.elements[id].focus().is_some()
    }

    fn tab_stops(&self) -> Vec<ElementId> {
        let scoped = |id: &ElementId| self.elements[*id].flags.focus_scope;
        let scope = if let Some(focused) = self.focused {
            self.chain(focused).into_iter().rev().find(scoped)
        } else {
            let mut found = Vec::new();
            self.collect(self.root, &mut |id| scoped(&id), &mut found);
            found.pop()
        };
        let mut stops = Vec::new();
        let stop = |id: ElementId| {
            self.elements[id]
                .focus()
                .is_some_and(|focus| focus.tab_index >= 0)
        };
        self.collect(scope.unwrap_or(self.root), &mut |id| stop(id), &mut stops);
        stops.sort_by_key(
            |id| match self.elements[*id].focus().map_or(0, |f| f.tab_index) {
                0 => i32::MAX,
                index => index,
            },
        );
        stops
    }

    fn collect(
        &self,
        id: ElementId,
        wanted: &mut impl FnMut(ElementId) -> bool,
        out: &mut Vec<ElementId>,
    ) {
        let element = &self.elements[id];
        if element.hidden() || element.disabled() {
            return;
        }
        if wanted(id) {
            out.push(id);
        }
        for child in &element.children {
            self.collect(*child, wanted, out);
        }
    }
}
