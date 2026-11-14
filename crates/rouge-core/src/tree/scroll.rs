use super::layers::Slot;
use super::{Element, ElementId, Tree};
use crate::compositor::{LayerId, ScrollState};
use crate::style::Dirty;
use rouge_reactive::Signal;
use rouge_render::{Point, Size};
use std::mem::take;

#[derive(Default)]
pub(crate) struct ScrollSpec {
    pub observer: Option<Signal<ScrollState>>,
    pub(super) published: Point,
    pub(super) target: Option<Point>,
    pub(super) extent: (Size, Size),
}

impl Element {
    pub(super) fn scroll(&self) -> Option<&ScrollSpec> {
        self.extras()?.scroll.as_ref()
    }

    pub(super) fn scroll_mut(&mut self) -> Option<&mut ScrollSpec> {
        self.extras.as_mut()?.scroll.as_mut()
    }
}

impl Tree {
    pub(crate) fn scroll_to(&mut self, id: ElementId, offset: Point) {
        let Some(scroll) = self.elements.get_mut(id).and_then(Element::scroll_mut) else {
            return;
        };
        if scroll.published == offset {
            return;
        }
        scroll.target = Some(offset);
        self.scrolled.push(id);
        if let Some(layer) = self.layer_id(id, Slot::Content) {
            self.compositor.scroll_to(layer, offset);
            self.mark(Dirty::Composite);
        }
    }

    pub(crate) fn scroll_targets(&self) -> Vec<LayerId> {
        let path = self.hover_path.iter().rev();
        path.filter_map(|id| Some(self.elements.get(*id)?.layer(Slot::Content)?.id))
            .collect()
    }

    pub(crate) fn scroll_by(&mut self, layer: LayerId, delta: Point) {
        self.compositor.scroll_by(layer, delta);
        self.mark(Dirty::Composite);
    }

    pub(crate) fn scroll_drag(&mut self, layer: LayerId, delta: Point) {
        self.compositor.scroll_drag(layer, delta);
        self.mark(Dirty::Composite);
    }

    pub(crate) fn scroll_release(&mut self, layer: LayerId, velocity: Point) {
        self.compositor.scroll_release(layer, velocity);
        self.mark(Dirty::Composite);
    }

    pub(super) fn publish_scroll(&mut self, id: ElementId) {
        let Some(element) = self.elements.get(id) else {
            return;
        };
        let layer = element.layer(Slot::Content).filter(|_| {
            element
                .scroll()
                .is_some_and(|scroll| scroll.target.is_none())
        });
        let Some(state) = layer.and_then(|layer| self.compositor.scroll(layer.id)) else {
            return;
        };
        if let Some(scroll) = self.elements[id].scroll_mut() {
            scroll.published = state.offset;
            if let Some(observer) = scroll.observer.filter(|o| o.is_alive()) {
                observer.set_if_changed(state);
            }
        }
    }

    pub(super) fn settle_scroll_targets(&mut self) {
        for id in take(&mut self.scrolled) {
            if let Some(scroll) = self.elements.get_mut(id).and_then(Element::scroll_mut) {
                scroll.target = None;
            }
        }
    }
}
