use super::layers::Slot;
use super::{ElementId, Interaction, STATES, Tree};
use crate::style::{Declaration, Dirty, Style, TextProperties};
use crate::text::TextStyle;
use rouge_render::Color;
use std::mem::replace;
use taffy::{NodeId, Style as Layout, TaffyTree};

impl Tree {
    pub(crate) fn update_style(&mut self, id: ElementId, declaration: Declaration) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let old = element.effective_style().clone();
        element.style.apply(&declaration);
        let mut relayout = false;
        if declaration.is_layout() {
            let base = element
                .extras
                .as_deref_mut()
                .and_then(|extras| extras.layout.as_deref_mut());
            if let Some(base) = base {
                declaration.lay_out(base);
            } else {
                let mut layout = self
                    .taffy
                    .style(element.taffy)
                    .expect("taffy style")
                    .clone();
                declaration.lay_out(&mut layout);
                relayout = set_layout(&mut self.taffy, element.taffy, layout);
            }
        }
        self.apply_style(id, &old, relayout);
    }

    pub(crate) fn update_variant(
        &mut self,
        id: ElementId,
        slot: usize,
        index: usize,
        declaration: Declaration,
    ) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let old = element.effective_style().clone();
        let variant = element
            .extras
            .as_deref_mut()
            .and_then(|extras| extras.variant_mut(slot));
        if let Some(entry) = variant.and_then(|variant| variant.get_mut(index)) {
            *entry = declaration;
        }
        self.apply_style(id, &old, false);
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
        let old = self.elements[id].effective_style().clone();
        self.apply_style(id, &old, false);
    }

    fn apply_style(&mut self, id: ElementId, old: &Style, mut relayout: bool) {
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
            declarations().for_each(|declaration| declaration.lay_out(&mut layout));
            relayout |= set_layout(&mut self.taffy, element.taffy, layout);
        }
        element.resolved = resolved;
        let new = element.effective_style();
        let text = !TextProperties::same(new, old);
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
    }

    fn active_variants(&self, id: ElementId) -> Vec<usize> {
        let element = &self.elements[id];
        let Some(extras) = element.extras() else {
            return Vec::new();
        };
        let disabled = element.disabled();
        let mut active: Vec<usize> = (0..STATES)
            .filter(|slot| element.states[*slot] && !extras.variants[*slot].is_empty())
            .filter(|slot| !disabled || *slot == Interaction::Disabled as usize)
            .collect();
        if extras.groups.is_empty() || self.is_disabled(id) {
            return active;
        }
        for (index, (name, _)) in extras.groups.iter().enumerate() {
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
            style.line_height = properties.text_line_height.unwrap_or(default.line_height);
            let color = properties.text_color.unwrap_or_else(|| {
                text.theme.map_or(Color::BLACK, |theme| {
                    theme.with_untracked(|palette| palette.text)
                })
            });
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

fn set_layout(taffy: &mut TaffyTree<ElementId>, node: NodeId, layout: Layout) -> bool {
    if *taffy.style(node).expect("taffy style") == layout {
        return false;
    }
    taffy.set_style(node, layout).expect("taffy set style");
    true
}
