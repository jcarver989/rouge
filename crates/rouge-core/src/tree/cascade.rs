use super::layers::Slot;
use super::{Element, ElementId, Interaction, PendingStyle, STATES, Tree, Trigger};
use crate::style::{Declaration, Dirty, TextProperties};
use crate::text::TextStyle;
use crate::theme::Fill;
use rouge_theme::Role;
use std::mem::{replace, take};

impl Tree {
    fn touch(&mut self, id: ElementId) -> Option<&mut Element> {
        let element = self.elements.get_mut(id)?;
        if element.pending.is_none() {
            element.pending = Some(Box::new(PendingStyle {
                old: element.effective_style().clone(),
                relayout: false,
            }));
            self.dirty_styles.push(id);
        }
        Some(element)
    }

    pub(crate) fn update_style(&mut self, id: ElementId, declaration: Declaration) {
        let Some(element) = self.touch(id) else {
            return;
        };
        element.style.apply(&declaration);
        if !declaration.is_layout() {
            return;
        }
        let base = element
            .extras
            .as_deref_mut()
            .and_then(|extras| extras.layout.as_deref_mut());
        if let Some(base) = base {
            declaration.lay_out(base);
        } else if declaration.lay_out(&mut self.layouts[id].style)
            && let Some(pending) = self.elements[id].pending.as_deref_mut()
        {
            pending.relayout = true;
        }
    }

    pub(crate) fn update_variant(
        &mut self,
        id: ElementId,
        slot: usize,
        index: usize,
        declaration: Declaration,
    ) {
        let Some(element) = self.touch(id) else {
            return;
        };
        let variant = element
            .extras
            .as_deref_mut()
            .and_then(|extras| extras.variant_mut(slot));
        if let Some(entry) = variant.and_then(|variant| variant.get_mut(index)) {
            *entry = declaration;
        }
    }

    pub(crate) fn resolve_styles(&mut self) {
        while !self.dirty_styles.is_empty() {
            let mut dirty = take(&mut self.dirty_styles);
            dirty.retain(|id| self.elements.get(*id).is_some_and(|e| e.pending.is_some()));
            dirty.sort_by_cached_key(|id| self.depth(*id));
            for id in &dirty {
                self.resolve(*id, false);
            }
            dirty.clear();
            if self.dirty_styles.is_empty() {
                self.dirty_styles = dirty;
            }
        }
    }

    pub(crate) fn resolve_new(&mut self, id: ElementId) {
        self.resolve(id, true);
    }

    fn depth(&self, id: ElementId) -> usize {
        let mut depth = 0;
        let mut parent = self.elements[id].parent;
        while let Some(id) = parent {
            depth += 1;
            parent = self.elements[id].parent;
        }
        depth
    }

    pub(crate) fn refresh_text_subtree(&mut self, id: ElementId) {
        let mut inherited = TextProperties::default();
        let mut parent = self.elements.get(id).and_then(|element| element.parent);
        while let Some(id) = parent {
            let element = &self.elements[id];
            inherited.inherit(element.effective_style());
            parent = element.parent;
        }
        self.refresh_text(id, &inherited);
    }

    pub(crate) fn set_condition(&mut self, id: ElementId, index: usize, on: bool) {
        let trigger = self
            .elements
            .get_mut(id)
            .and_then(|element| element.extras.as_deref_mut())
            .and_then(|extras| extras.triggers.get_mut(index));
        if let Some((Trigger::When(current), _)) = trigger
            && *current != on
        {
            *current = on;
            self.refresh_variant(id);
        }
    }

    pub(super) fn set_interaction(&mut self, id: ElementId, which: Interaction, on: bool) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let unchanged = replace(&mut element.states[which as usize], on) == on;
        if unchanged {
            return;
        }
        let grouped = element
            .extras()
            .is_some_and(|extras| extras.group.is_some());
        match which {
            Interaction::Selected | Interaction::Checked => self.mark_access(id),
            Interaction::Disabled => self.mark_access_subtree(id),
            _ => {}
        }
        if grouped || matches!(which, Interaction::Disabled) {
            self.refresh_variant_subtree(id);
        } else {
            self.refresh_variant(id);
        }
    }

    pub(super) fn refresh_variant_subtree(&mut self, id: ElementId) {
        self.refresh_variant(id);
        let children = self.elements[id].children.clone();
        for child in children {
            self.refresh_variant_subtree(child);
        }
    }

    fn refresh_variant(&mut self, id: ElementId) {
        let varies = self
            .elements
            .get(id)
            .and_then(Element::extras)
            .is_some_and(|extras| {
                !extras.triggers.is_empty()
                    || extras.variants.iter().any(|variant| !variant.is_empty())
            });
        if varies {
            self.touch(id);
        }
    }

    fn resolve(&mut self, id: ElementId, fresh: bool) {
        let Some(pending) = self.elements.get_mut(id).and_then(|e| e.pending.take()) else {
            if fresh {
                self.sync_backing(id);
            }
            return;
        };
        let PendingStyle { old, mut relayout } = *pending;
        let old = &old;
        self.stats.style_resolutions += 1;
        let active = self.active_variants(id);
        let element = &mut self.elements[id];
        let extras = element.extras.as_deref();
        let declarations = || {
            active
                .iter()
                .filter_map(|slot| extras.and_then(|extras| extras.variant(*slot)))
                .flatten()
        };
        let resolved = (!active.is_empty()).then(|| {
            let mut style = element.style.clone();
            declarations().for_each(|declaration| style.apply(declaration));
            Box::new(style)
        });
        if let Some(base) = extras.and_then(|extras| extras.layout.as_deref()) {
            let mut layout = base.clone();
            declarations().for_each(|declaration| {
                declaration.lay_out(&mut layout);
            });
            if self.layouts[id].style != layout {
                self.layouts[id].style = layout;
                relayout = true;
            }
        }
        element.resolved = resolved;
        if relayout {
            self.invalidate_layout(id);
        }
        if fresh {
            self.sync_backing(id);
            return;
        }
        let element = &self.elements[id];
        let new = element.effective_style();
        let text = !TextProperties::same(new, old);
        let reshown = new.hidden != old.hidden || new.invisible != old.invisible;
        let dirty = if relayout {
            Dirty::Layout
        } else if new.same_box(old) {
            Dirty::None
        } else if new.border_color != old.border_color
            || new.shadows != old.shadows
            || new.invisible != old.invisible
            || new.background.is_some() != old.background.is_some()
            || element.layer(Slot::Own).is_none()
        {
            Dirty::Paint
        } else {
            Dirty::Composite
        };
        if dirty != Dirty::None {
            self.mark_element(id, dirty);
            self.sync_backing(id);
        }
        if text {
            self.refresh_text_subtree(id);
        }
        if reshown {
            self.mark_around(id);
            self.mark_access_subtree(id);
        }
    }

    fn active_variants(&self, id: ElementId) -> Vec<usize> {
        let element = &self.elements[id];
        let Some(extras) = element.extras() else {
            return Vec::new();
        };
        let disabled = element.disabled();
        let mut active: Vec<usize> = (0..STATES)
            .filter(|slot| element.states[*slot] && !extras.variants[*slot].is_empty())
            .filter(|slot| !disabled || !Interaction::ALL[*slot].pointer())
            .collect();
        if extras.triggers.is_empty() {
            return active;
        }
        let disabled = self.is_disabled(id);
        for (index, (trigger, _)) in extras.triggers.iter().enumerate() {
            let name = match trigger {
                Trigger::When(on) => {
                    if *on {
                        active.push(STATES + index);
                    }
                    continue;
                }
                Trigger::GroupHover(_) if disabled => continue,
                Trigger::GroupHover(name) => name,
            };
            let mut parent = element.parent;
            while let Some(id) = parent {
                let ancestor = &self.elements[id];
                let group = ancestor.extras().and_then(|extras| extras.group.as_ref());
                if group == Some(name)
                    && ancestor.states[Interaction::Hover as usize]
                    && !ancestor.disabled()
                {
                    active.push(STATES + index);
                    break;
                }
                parent = ancestor.parent;
            }
        }
        active
    }

    fn refresh_text(&mut self, id: ElementId, inherited: &TextProperties) {
        let Some(element) = self.elements.get(id) else {
            return;
        };
        let properties = inherited.cascade(element.effective_style());
        let children = element.children.clone();
        if let Some(text) = element.kind.text() {
            let default = TextStyle::default();
            let mut style = text.style.clone();
            style.size = properties.text_size.unwrap_or(default.size);
            style.family = properties.text_font.clone().unwrap_or(default.family);
            style.weight = properties.text_weight.unwrap_or(default.weight);
            style.italic = properties.text_italic.unwrap_or(default.italic);
            style.strikethrough = properties
                .text_strikethrough
                .unwrap_or(default.strikethrough);
            style.line_height = properties.text_line_height.unwrap_or(default.line_height);
            let color = properties.text_color.unwrap_or(Fill::Role(Role::Text));
            let dirty = if style != text.style {
                Dirty::Layout
            } else if color != text.color {
                Dirty::Paint
            } else {
                Dirty::None
            };
            if dirty != Dirty::None {
                self.update_text(id, dirty, |text| {
                    text.style = style;
                    text.color = color;
                });
            }
        }
        for child in children {
            self.refresh_text(child, &properties);
        }
    }
}
