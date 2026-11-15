use super::{Kind, Tree};
use crate::surface::SurfaceOp;
use rouge_render::{SurfaceConfig, Surfaces};
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
                && !element.unseen()
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

    pub(crate) fn sync_surfaces(&mut self, surfaces: &mut Surfaces) {
        for op in take(&mut self.surface_ops) {
            match op {
                SurfaceOp::Attach(source, status) => surfaces.attach(source, status),
                SurfaceOp::Place(id, placement) => surfaces.place(id, placement),
                SurfaceOp::Detach(id) => surfaces.detach(id),
            }
        }
        self.compositor.sync_surfaces(surfaces);
    }

    pub(crate) fn set_window_visible(&mut self, visible: bool) {
        self.window_visible = visible;
    }
}
