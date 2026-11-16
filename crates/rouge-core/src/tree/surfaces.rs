use super::{Kind, Tree};
use crate::compositor::SurfaceOp;
use rouge_render::{SurfaceConfig, SurfaceId, SurfaceStatus};
use std::mem::take;

impl Tree {
    pub(crate) fn place_surfaces(&mut self, scale: f32) {
        for index in 0..self.surfaces.len() {
            let id = self.surfaces[index];
            let Some(element) = self.elements.get(id) else {
                continue;
            };
            let size = element.bounds.size;
            let shown = self.window_visible
                && !self
                    .chain(id)
                    .iter()
                    .any(|node| self.elements[*node].unseen())
                && !self.paint_region(id).bounds.is_empty();
            let placement = SurfaceConfig {
                width: (size.width * scale).round().max(0.0) as u32,
                height: (size.height * scale).round().max(0.0) as u32,
                scale,
                visible: shown,
            };
            let Kind::Surface(surface) = &mut self.elements[id].kind else {
                continue;
            };
            if surface.placement.replace(placement) != Some(placement) {
                self.surface_ops
                    .push(SurfaceOp::Place(surface.id, placement));
            }
        }
    }

    pub(crate) fn take_surface_ops(&mut self) -> Vec<SurfaceOp> {
        take(&mut self.surface_ops)
    }

    pub(crate) fn surface_status(&self, id: SurfaceId, status: SurfaceStatus) {
        if let Some(sink) = self.surface_sinks.get(&id) {
            sink(status);
        }
    }

    pub(crate) fn set_window_visible(&mut self, visible: bool) {
        self.window_visible = visible;
    }
}
