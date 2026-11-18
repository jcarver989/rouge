use super::layers::Slot;
use super::{ElementId, Tree};
use crate::canvas::{PaintOp, TextHit};
use crate::event::DragAxis;
use rouge_reactive::batch;
use rouge_render::{Affine, Point, Rect, Size};
use std::borrow::Cow;
use std::iter::{from_fn, successors};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaintRegion {
    pub bounds: Rect,
    pub order: Vec<usize>,
}

impl Tree {
    pub(crate) fn publish(&mut self) {
        if !self.dirty.needs_layout() {
            self.settle_scroll_targets();
        }
        batch(|| {
            self.publish_motions();
            for index in 0..self.observers.len() {
                self.publish_observers(self.observers[index]);
            }
        });
    }

    pub(crate) fn following(&self) -> bool {
        self.layers.is_animating() && self.observers.iter().any(|id| self.follows(*id))
    }

    pub(crate) fn publish_motions(&self) {
        batch(|| {
            for (id, value) in &self.motions {
                if let Some(current) = self.layers.motion(*id).filter(|_| value.is_alive()) {
                    value.set_if_changed(current);
                }
            }
        });
    }

    pub(crate) fn named_bounds(&self) -> Vec<(String, Rect)> {
        self.visible_preorder(self.root)
            .filter_map(|id| Some((self.test_id(id)?.into_owned(), self.window_bounds(id))))
            .collect()
    }

    pub(crate) fn bounds_of_all(&self, name: &str) -> Vec<Rect> {
        let mut bounds = Vec::new();
        for id in self.visible_preorder(self.root) {
            let element = &self.elements[id];
            if self.test_id(id).as_deref() == Some(name) {
                bounds.push(self.window_bounds(id));
            }
            let ops = element
                .kind
                .canvas()
                .into_iter()
                .flat_map(|canvas| &canvas.ops);
            for op in ops {
                let rect = match op {
                    PaintOp::Region { name: region, rect } if region == name => *rect,
                    PaintOp::Text(run) if run.run.name.as_deref() == Some(name) => run.bounds(),
                    _ => continue,
                };
                let rect = rect.translate(element.bounds.origin);
                bounds.push(self.to_window(id, rect));
            }
        }
        bounds
    }

    pub(crate) fn text_of(&self, name: &str) -> Option<String> {
        let found = self
            .visible_preorder(self.root)
            .find(|id| self.test_id(*id).as_deref() == Some(name))?;
        Some(
            self.visible_preorder(found)
                .filter_map(|id| self.elements[id].kind.text())
                .map(|data| data.content.as_str())
                .collect(),
        )
    }

    pub(crate) fn text_bounds(&self, needle: &str) -> Option<Rect> {
        self.visible_preorder(self.root).find_map(|id| {
            let element = &self.elements[id];
            let text = element.kind.text()?;
            let start = text.content.find(needle)?;
            let layout = text.layout.as_ref()?;
            let rect = layout
                .selection_rects(layout.select(start, start + needle.len()))
                .into_iter()
                .next()?;
            let origin = element.bounds.origin + text.inset;
            let local = Rect::new(
                origin.x + rect.x,
                origin.y + rect.y,
                rect.width,
                rect.height,
            );
            Some(self.to_window(id, local))
        })
    }

    pub(crate) fn text_hit(&self, id: ElementId, point: Point) -> Option<TextHit> {
        let kind = &self.elements.get(id)?.kind;
        let local = self.to_local(id, point);
        if let Some(text) = kind.text() {
            let point = local - text.inset;
            let index = text.layout.as_ref()?.index_at(point.x, point.y)?;
            return Some(TextHit { tag: 0, index });
        }
        kind.canvas()?.ops.iter().rev().find_map(|op| match op {
            PaintOp::Text(run) => run.hit(local),
            _ => None,
        })
    }

    pub(crate) fn to_local(&self, id: ElementId, point: Point) -> Point {
        let Some(element) = self.elements.get(id) else {
            return point;
        };
        let transforms = self.transforms(id);
        let inverses = transforms.iter().filter_map(Affine::inverse);
        inverses.fold(point, |point, inverse| inverse.apply(point)) - element.bounds.origin
    }

    pub(crate) fn drag_axis(&self, id: ElementId) -> Option<DragAxis> {
        self.elements.get(id)?.flags.drag_axis
    }

    pub(crate) fn size_of(&self, id: ElementId) -> Size {
        self.elements
            .get(id)
            .map_or(Size::ZERO, |element| element.bounds.size)
    }

    pub(crate) fn window_bounds(&self, id: ElementId) -> Rect {
        let Some(element) = self.elements.get(id) else {
            return Rect::ZERO;
        };
        self.to_window(id, element.bounds)
    }

    pub(super) fn paint_region(&self, id: ElementId) -> PaintRegion {
        let chain = self.chain(id);
        let mut bounds = self.window_bounds(id).intersect(&Rect::new(
            0.0,
            0.0,
            self.size.width,
            self.size.height,
        ));
        let mut order = Vec::with_capacity(chain.len());
        for (i, &node) in chain.iter().enumerate() {
            let element = &self.elements[node];
            if element.hidden() || bounds.is_empty() {
                return PaintRegion::default();
            }
            let Some(child) = chain.get(i + 1) else {
                break;
            };
            let Some(index) = element.children.iter().position(|id| id == child) else {
                return PaintRegion::default();
            };
            order.push(index);
            if element.clips_children() {
                let r = element.bounds;
                let b = element.border;
                let clip = Rect::new(
                    r.origin.x + b.left,
                    r.origin.y + b.top,
                    r.size.width - b.left - b.right,
                    r.size.height - b.top - b.bottom,
                );
                bounds = bounds.intersect(&self.to_window(node, clip));
            }
        }
        PaintRegion { bounds, order }
    }

    pub(super) fn to_window(&self, id: ElementId, rect: Rect) -> Rect {
        let transforms = self.transforms(id);
        transforms
            .iter()
            .rev()
            .fold(rect, |rect, transform| transform.apply_rect(&rect))
    }

    fn test_id(&self, id: ElementId) -> Option<Cow<'_, str>> {
        let own = self.elements[id].extras()?.test_id.as_deref()?;
        let mut parts: Vec<&str> =
            successors(self.elements[id].parent, |id| self.elements[*id].parent)
                .filter_map(|id| {
                    self.elements[id]
                        .extras()
                        .filter(|extras| extras.test_scope)
                })
                .filter_map(|extras| extras.test_id.as_deref())
                .collect();
        if parts.is_empty() {
            return Some(Cow::Borrowed(own));
        }
        parts.reverse();
        parts.push(own);
        Some(Cow::Owned(parts.join("-")))
    }

    fn enter(&self, id: ElementId, slot: Slot) -> Option<Affine> {
        let element = &self.elements[id];
        let pose = self.layers.pose(element.layer(slot)?.id)?;
        Some(pose.transform(element.bounds))
    }

    fn transforms(&self, id: ElementId) -> Vec<Affine> {
        let chain = self.chain(id);
        let steps = chain.iter().enumerate().flat_map(|(index, element)| {
            let own = self.enter(*element, Slot::Own);
            let content = self
                .enter(*element, Slot::Content)
                .filter(|_| index + 1 < chain.len());
            [own, content]
        });
        steps.flatten().collect()
    }

    fn visible_preorder(&self, from: ElementId) -> impl Iterator<Item = ElementId> + '_ {
        let mut pending = vec![from];
        from_fn(move || {
            while let Some(id) = pending.pop() {
                let element = &self.elements[id];
                if !element.hidden() {
                    pending.extend(element.children.iter().rev());
                    return Some(id);
                }
            }
            None
        })
    }

    fn follows(&self, id: ElementId) -> bool {
        successors(Some(id), |id| self.elements.get(*id)?.parent)
            .take(self.reach(id))
            .filter_map(|id| self.elements.get(id))
            .flat_map(|element| element.layers.iter().flatten())
            .any(|layer| self.layers.is_layer_moving(layer.id))
    }

    fn reach(&self, id: ElementId) -> usize {
        let Some(element) = self.elements.get(id) else {
            return 0;
        };
        let bounds = element.extras().is_some_and(|extras| {
            extras.bounds_observer.is_some() || extras.paint_region.is_some()
        });
        let tracked_text = element.kind.canvas().is_some_and(|canvas| {
            canvas
                .ops
                .iter()
                .any(|op| matches!(op, PaintOp::Text(shaped) if shaped.run.track.is_some()))
        });
        let scroll = element
            .scroll()
            .is_some_and(|scroll| scroll.observer.is_some());
        if bounds || tracked_text {
            usize::MAX
        } else {
            usize::from(scroll)
        }
    }

    fn publish_observers(&mut self, id: ElementId) {
        let Some(element) = self.elements.get(id) else {
            return;
        };
        if let Some(extras) = element.extras() {
            if let Some(observer) = extras.size_observer.filter(|o| o.is_alive()) {
                observer.set_if_changed(element.bounds.size);
            }
            if let Some(observer) = extras.bounds_observer.filter(|o| o.is_alive()) {
                observer.set_if_changed(self.window_bounds(id));
            }
            if let Some(observer) = extras.paint_region.filter(|o| o.is_alive()) {
                observer.set_if_changed(self.paint_region(id));
            }
        }
        if let Some(canvas) = element.kind.canvas() {
            for op in &canvas.ops {
                if let PaintOp::Text(shaped) = op
                    && let Some((observer, rect)) = shaped.tracked()
                    && observer.is_alive()
                {
                    let local = Rect::new(
                        element.bounds.origin.x + rect.origin.x,
                        element.bounds.origin.y + rect.origin.y,
                        rect.size.width,
                        rect.size.height,
                    );
                    observer.set_if_changed(self.to_window(id, local));
                }
            }
        }
        let edit = element.kind.text().and_then(|text| text.edit.as_ref());
        if let Some(edit) = edit
            && let Some(observer) = edit.selection_signal.filter(|o| o.is_alive())
        {
            observer.set_if_changed(edit.selection.range());
        }
        self.publish_scroll(id);
    }
}
