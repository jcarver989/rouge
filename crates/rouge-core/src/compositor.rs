mod actor;
mod commit;
mod layers;
mod link;
mod model;
#[cfg(not(target_arch = "wasm32"))]
mod thread;

use model::{Applied, Layer};
use rouge_render::{
    Affine, Command, Corners, DrawList, DrawLists, EffectDraw, Group, LayerDraw, ListKey, Point,
    Rect, Scene, SurfaceDraw, Surfaces, device_rect,
};
use rustc_hash::FxHasher;
use slotmap::secondary::Entry;
use slotmap::{Key, SecondaryMap};
use std::hash::Hasher;
use std::mem::take;
use tracing::trace_span;

pub use actor::{CompositorActor, Message, WindowConfig, WindowId};
pub use commit::{Commit, CompositorEvent, SurfaceOp};
pub use layers::Layers;
pub use link::{Events, Mailbox, mailbox};
pub use model::{
    EffectSlot, LayerChanges, LayerDelta, LayerId, LayerModel, LayerPose, MotionId, Nested,
    ScrollLinked, ScrollState,
};
#[cfg(not(target_arch = "wasm32"))]
pub use thread::{ThreadError, spawn};

const STAGE_BITS: u32 = 8;

#[derive(Default)]
pub struct Content {
    pub lists: Vec<DrawList>,
    pub nested: Vec<Nested>,
}

#[derive(Default)]
pub(crate) struct Compositor {
    model: LayerModel,
    lists: DrawLists,
    keys: SecondaryMap<LayerId, Vec<ListKey>>,
    commands: Vec<Command>,
}

impl Compositor {
    pub(crate) fn model(&self) -> &LayerModel {
        &self.model
    }

    pub(crate) fn apply(
        &mut self,
        layers: LayerDelta,
        content: Vec<(LayerId, Vec<DrawList>)>,
        surfaces: &mut Surfaces,
    ) {
        let changes = self.model.apply(layers);
        for id in changes.removed {
            for key in self.keys.remove(id).into_iter().flatten() {
                self.lists.remove(key);
            }
        }
        for (id, lists) in content {
            self.set_lists(id, lists);
        }
        for (id, bound) in changes.surfaces {
            if bound {
                surfaces.bind(id);
            } else {
                surfaces.unbind(id);
            }
        }
    }

    fn set_lists(&mut self, id: LayerId, content: Vec<DrawList>) {
        if self.model.layer(id).is_none() {
            return;
        }
        let Some(keys) = self.keys.entry(id).map(Entry::or_default) else {
            return;
        };
        let lists = &mut self.lists;
        for key in keys.drain(content.len().min(keys.len())..) {
            lists.remove(key);
        }
        for (index, list) in content.into_iter().enumerate() {
            match keys.get(index).and_then(|key| lists.get_mut(*key)) {
                Some(existing) => existing.replace(list),
                None => keys.push(lists.insert(list)),
            }
        }
    }

    pub(crate) fn scene(&mut self, scale: f32, surfaces: &Surfaces, at: f64) -> Scene<'_> {
        let _scene = trace_span!("scene").entered();
        self.build(scale, surfaces, at.max(self.model.time()));
        Scene::new(&self.commands, &self.lists)
    }

    fn build(&mut self, scale: f32, surfaces: &Surfaces, at: f64) {
        let mut commands = take(&mut self.commands);
        commands.clear();
        if let Some(root) = self.model.root() {
            let parent = Parent {
                transform: Affine::IDENTITY,
                opacity: 1.0,
                scissor: None,
            };
            let mut out = Out {
                commands: &mut commands,
                surfaces,
                scale,
                at,
            };
            self.emit(root, None, &parent, &mut out);
        }
        self.commands = commands;
    }

    fn emit(&self, id: LayerId, clip: Option<Rect>, parent: &Parent, out: &mut Out<'_>) {
        let Some(layer) = self.model.layer(id) else {
            return;
        };
        let at = out.at;
        if layer.exited(at) {
            return;
        }
        let pose = self.model.linked_pose(layer, at);
        if pose.opacity <= 0.0 {
            return;
        }
        let scale = out.scale;
        let scissor = match clip {
            Some(clip) => {
                let clip = device_rect(&parent.transform, &clip, scale);
                Some(
                    parent
                        .scissor
                        .map_or(clip, |scissor| scissor.intersect(&clip)),
                )
            }
            None => parent.scissor,
        };
        if scissor.is_some_and(|scissor| scissor.is_empty()) {
            return;
        }
        let transform = layer.local(&pose).then(&parent.transform);
        let radii = layer.radii(self.model.motions(), at);
        let shaped = |applied: &Applied| EffectDraw {
            effect: applied.instance(at),
            bounds: layer.bounds(),
            corner_radii: radii,
        };
        let style = Style {
            fill: layer.fill(self.model.motions(), at),
            radii,
            backdrop: layer.effects.get(&EffectSlot::Backdrop).map(shaped),
        };
        let stages = layer.stages().count();
        let grouped = stages > 0
            || ((layer.rasterize || (pose.opacity < 1.0 && self.overlaps(id, layer)))
                && !self.has_nested_backdrop(layer));
        let opacity = pose.opacity * parent.opacity;
        if !grouped {
            let this = Parent {
                transform,
                opacity,
                scissor,
            };
            self.emit_content(id, layer, style, &this, out);
            return;
        }
        let offset = (
            (transform.tx * scale).round() as i32,
            (transform.ty * scale).round() as i32,
        );
        let shift = Point::new(offset.0 as f32, offset.1 as f32);
        let local = Affine {
            tx: (transform.tx * scale - shift.x) / scale,
            ty: (transform.ty * scale - shift.y) / scale,
            ..transform
        };
        let local_scissor =
            scissor.map(|scissor| scissor.translate(Point::new(-shift.x, -shift.y)));
        if style.backdrop.is_some() {
            out.commands.push(Command::Draw(LayerDraw {
                transform,
                opacity,
                scissor,
                backdrop: style.backdrop,
                ..LayerDraw::default()
            }));
        }
        let groups = stages.max(1);
        let first = out.commands.len();
        for stage in (0..groups).rev() {
            let outer = stage + 1 == groups;
            out.commands.push(Command::Group(Group {
                key: (id.data().as_ffi() << STAGE_BITS) | stage as u64,
                signature: 0,
                bounds: Rect::ZERO,
                offset: if outer { offset } else { (0, 0) },
                opacity: if outer { opacity } else { 1.0 },
                scissor: if outer { scissor } else { local_scissor },
                transform: if outer { transform } else { local },
                effect: layer.stages().nth(stage).map(shaped),
                len: 0,
            }));
        }
        let this = Parent {
            transform: local,
            opacity: 1.0,
            scissor: local_scissor,
        };
        let content = Style {
            backdrop: None,
            ..style
        };
        self.emit_content(id, layer, content, &this, out);
        for index in (first..first + groups).rev() {
            let end = out.commands.len();
            let mut hasher = FxHasher::default();
            let scene = Scene::new(out.commands, &self.lists);
            let Some(bounds) = scene.summarize(index + 1..end, scale, &mut hasher) else {
                out.commands.truncate(first);
                return;
            };
            if let Command::Group(group) = &mut out.commands[index] {
                group.signature = hasher.finish();
                group.bounds = bounds;
                group.len = (end - index - 1) as u32;
            }
        }
    }

    fn emit_content(
        &self,
        id: LayerId,
        layer: &Layer,
        mut style: Style,
        this: &Parent,
        out: &mut Out<'_>,
    ) {
        let mut surface = layer.surface.map(|id| {
            Command::Surface(SurfaceDraw {
                id,
                frame: out.surfaces.frame(id),
                bounds: layer.bounds(),
                transform: this.transform,
                opacity: this.opacity,
                radii: style.radii,
                scissor: this.scissor,
            })
        });
        for (segment, key) in self.lists_of(id).iter().enumerate() {
            let empty = self.lists.get(*key).is_none_or(DrawList::is_empty);
            if !empty || style.backdrop.is_some() {
                out.commands.push(Command::Draw(LayerDraw {
                    list: Some(*key),
                    transform: this.transform,
                    opacity: this.opacity,
                    fill: style.fill,
                    radii: style.radii,
                    backdrop: style.backdrop.take(),
                    scissor: this.scissor,
                }));
            }
            out.commands.extend(surface.take());
            if let Some(nested) = layer.nested.get(segment) {
                self.emit(nested.layer, nested.clip, this, out);
            }
        }
        out.commands.extend(surface);
        for orphan in &layer.orphans {
            self.emit(*orphan, None, this, out);
        }
    }

    fn lists_of(&self, id: LayerId) -> &[ListKey] {
        self.keys.get(id).map_or(&[], Vec::as_slice)
    }

    fn children<'a>(&'a self, layer: &'a Layer) -> impl Iterator<Item = (LayerId, &'a Layer)> {
        let ids = layer
            .nested
            .iter()
            .map(|nested| &nested.layer)
            .chain(&layer.orphans);
        ids.filter_map(|id| Some((*id, self.model.layer(*id)?)))
    }

    fn primitives(&self, id: LayerId, layer: &Layer, limit: usize) -> usize {
        let lists = self
            .lists_of(id)
            .iter()
            .filter_map(|key| self.lists.get(*key));
        let mut count =
            lists.map(DrawList::len).sum::<usize>() + usize::from(layer.surface.is_some());
        for (child_id, child) in self.children(layer) {
            if count >= limit {
                break;
            }
            count += self.primitives(child_id, child, limit - count);
        }
        count
    }

    fn overlaps(&self, id: LayerId, layer: &Layer) -> bool {
        self.primitives(id, layer, 2) >= 2
    }

    fn has_nested_backdrop(&self, layer: &Layer) -> bool {
        let reads = |child: &Layer| child.effects.range(EffectSlot::Blend..).next().is_some();
        self.children(layer)
            .any(|(_, child)| reads(child) || self.has_nested_backdrop(child))
    }
}

struct Out<'a> {
    commands: &'a mut Vec<Command>,
    surfaces: &'a Surfaces,
    scale: f32,
    at: f64,
}

struct Parent {
    transform: Affine,
    opacity: f32,
    scissor: Option<Rect>,
}

#[derive(Clone, Copy)]
struct Style {
    fill: [f32; 4],
    radii: Corners,
    backdrop: Option<EffectDraw>,
}
