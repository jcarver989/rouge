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
use rustc_hash::{FxHashMap, FxHasher};
use slotmap::secondary::Entry;
use slotmap::{Key, SecondaryMap};
use std::hash::Hasher;
use std::mem::take;
use tracing::trace_span;

pub use actor::{CompositorActor, Message, WindowConfig, WindowId};
pub use commit::{Commit, CompositorEvent, Content, ListDelta, ListId, PlacedList, SurfaceOp};
pub use layers::Layers;
pub use link::{Events, Mailbox, mailbox};
pub use model::{
    EffectSlot, LayerChanges, LayerDelta, LayerId, LayerModel, LayerPose, MotionId, Nested,
    ScrollLinked, ScrollState,
};
#[cfg(not(target_arch = "wasm32"))]
pub use thread::{ThreadError, spawn};

const STAGE_BITS: u32 = 8;

struct Held {
    id: ListId,
    key: ListKey,
    segment: usize,
    offset: Point,
}

#[derive(Default)]
pub(crate) struct Compositor {
    model: LayerModel,
    lists: DrawLists,
    held: SecondaryMap<LayerId, Vec<Held>>,
    commands: Vec<Command>,
    built: Option<(f32, Option<f64>)>,
    surface_commands: Vec<usize>,
    grouped: bool,
    revision: u64,
}

impl Compositor {
    pub(crate) fn model(&self) -> &LayerModel {
        &self.model
    }

    pub(crate) fn apply(
        &mut self,
        layers: LayerDelta,
        content: Vec<(LayerId, ListDelta)>,
        surfaces: &mut Surfaces,
    ) {
        let root = self.model.root();
        let structural = !layers.is_empty();
        let changes = self.model.apply(layers);
        if structural || self.model.root() != root {
            self.built = None;
        }
        for id in changes.removed {
            for held in self.held.remove(id).into_iter().flatten() {
                self.lists.remove(held.key);
            }
        }
        for (id, lists) in content {
            match lists {
                ListDelta::Replace(lists) => {
                    self.set_lists(id, lists);
                    self.built = None;
                }
                ListDelta::Patch(patch) => self.patch_lists(id, patch),
            }
        }
        for (id, bound) in changes.surfaces {
            if bound {
                surfaces.bind(id);
            } else {
                surfaces.unbind(id);
            }
        }
    }

    fn patch_lists(&mut self, id: LayerId, patch: Vec<(usize, PlacedList)>) {
        let Some(held) = self.held.get_mut(id) else {
            return;
        };
        for (index, placed) in patch {
            let Some(slot) = held.get_mut(index).filter(|slot| slot.id == placed.id) else {
                continue;
            };
            if (slot.segment, slot.offset) != (placed.segment, placed.offset) {
                slot.segment = placed.segment;
                slot.offset = placed.offset;
                self.built = None;
            }
            if let Some(list) = placed.list
                && let Some(existing) = self.lists.get_mut(slot.key)
            {
                let revision = existing.revision();
                let resized = existing.len() != list.len();
                existing.replace(list);
                if existing.revision() != revision {
                    self.revision += 1;
                    if resized || self.grouped {
                        self.built = None;
                    }
                }
            }
        }
    }

    fn set_lists(&mut self, id: LayerId, content: Vec<PlacedList>) {
        if self.model.layer(id).is_none() {
            return;
        }
        let Some(held) = self.held.entry(id).map(Entry::or_default) else {
            return;
        };
        let lists = &mut self.lists;
        let mut known: FxHashMap<ListId, ListKey> =
            held.drain(..).map(|held| (held.id, held.key)).collect();
        for placed in content {
            let key = match (known.remove(&placed.id), placed.list) {
                (Some(key), Some(list)) => {
                    if let Some(existing) = lists.get_mut(key) {
                        existing.replace(list);
                    }
                    key
                }
                (Some(key), None) => key,
                (None, Some(list)) => lists.insert(list),
                (None, None) => {
                    debug_assert!(false, "rouge: kept a draw list the layer does not hold");
                    continue;
                }
            };
            held.push(Held {
                id: placed.id,
                key,
                segment: placed.segment,
                offset: placed.offset,
            });
        }
        for key in known.into_values() {
            lists.remove(key);
        }
    }

    pub(crate) fn scene(&mut self, scale: f32, surfaces: &Surfaces, at: f64) -> (u64, Scene<'_>) {
        let _scene = trace_span!("scene").entered();
        let at = at.max(self.model.time());
        let stamp = (scale, self.model.settles_at().map(|end| at.min(end)));
        if self.built == Some(stamp) {
            for &index in &self.surface_commands {
                if let Command::Surface(draw) = &mut self.commands[index] {
                    let frame = surfaces.frame(draw.id);
                    if draw.frame != frame {
                        if self.grouped {
                            self.built = None;
                            break;
                        }
                        draw.frame = frame;
                        self.revision += 1;
                    }
                }
            }
        }
        if self.built != Some(stamp) {
            self.build(scale, surfaces, at);
            self.built = Some(stamp);
            self.revision += 1;
        }
        (self.revision, Scene::new(&self.commands, &self.lists))
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
                grouped: false,
                commands: &mut commands,
                surfaces,
                scale,
                at,
            };
            self.emit(root, None, &parent, &mut out);
            self.grouped = out.grouped;
        }
        self.surface_commands.clear();
        if self.model.has_surfaces() {
            self.surface_commands
                .extend(commands.iter().enumerate().filter_map(|(index, command)| {
                    matches!(command, Command::Surface(_)).then_some(index)
                }));
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
        out.grouped = true;
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
        let mut lists = self.lists_of(id).iter().peekable();
        for segment in 0..=layer.nested.len() {
            while let Some(held) = lists.next_if(|held| held.segment <= segment) {
                let empty = self.lists.get(held.key).is_none_or(DrawList::is_empty);
                if !empty || style.backdrop.is_some() {
                    out.commands.push(Command::Draw(LayerDraw {
                        list: Some(held.key),
                        transform: held.transform(&this.transform),
                        opacity: this.opacity,
                        fill: style.fill,
                        radii: style.radii,
                        backdrop: style.backdrop.take(),
                        scissor: this.scissor,
                    }));
                }
                out.commands.extend(surface.take());
            }
            if let Some(nested) = layer.nested.get(segment) {
                self.emit(nested.layer, nested.clip, this, out);
            }
        }
        out.commands.extend(surface);
        for orphan in &layer.orphans {
            self.emit(*orphan, None, this, out);
        }
    }

    fn lists_of(&self, id: LayerId) -> &[Held] {
        self.held.get(id).map_or(&[], Vec::as_slice)
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
            .filter_map(|held| self.lists.get(held.key));
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

impl Held {
    fn transform(&self, layer: &Affine) -> Affine {
        if self.offset == Point::ZERO {
            *layer
        } else {
            Affine::translate(self.offset.x, self.offset.y).then(layer)
        }
    }
}

struct Out<'a> {
    grouped: bool,
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
