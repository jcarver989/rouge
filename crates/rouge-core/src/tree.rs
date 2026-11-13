use crate::canvas::{CanvasMeasure, HitListener, PaintOp, ShapedRun, TextHit};
use crate::clipboard::{Clipboard, MemoryClipboard};
use crate::event::{
    Chord, Cursor, DragEvent, FileDropEvent, FocusEvent, ImeEvent, KeyEvent, Modifiers,
    PointerEvent, ScrollEvent,
};
use crate::image::{Fit, ImageSpec};
use crate::keymap::Keymap;
use crate::path::Path;
use crate::scroll::scroll_key;
use crate::style::{Dirty, Style, TextProperties};
use crate::surface::{SurfaceOp, SurfaceSpec};
use crate::theme::Theme;
use edit::{CARET_WIDTH, EditState};
use rouge_reactive::{ReactiveStats, Signal, batch};
use rouge_render::{
    Affine, Atlas, AtlasRect, Color, Command, Commit, Compositor, Content, Corners, DrawList,
    Edges, Glyph, ImageData, ImageQuad, LayerCommand, LayerId, MotionCommand, MotionId, Nested,
    Placement, Point, Pose, Quad, Rect, ScrollState, Shadow, Size, Timing,
};
use rouge_text::{GlyphKey, TextLayout, TextRect, TextStats, TextStyle, TextSystem};
use slotmap::{SlotMap, new_key_type};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::iter::from_fn;
use std::mem::{replace, take};
use std::ops::Sub;
use std::rc::Rc;
use taffy::{
    AvailableSpace, Dimension, Display, NodeId, Overflow, Position, Rect as Sides, Size as Extent,
    TaffyTree, compute_leaf_layout,
};
use tracing::trace_span;
use zeno::{Cap, Fill, Join, Mask, Stroke, Transform};

pub(crate) mod edit;

new_key_type! {
    pub struct ElementId;
}

const WRAP_TOLERANCE: f32 = 1.0;

pub(crate) type Variant = Rc<dyn Fn(Style) -> Style>;
pub(crate) type ActionHandler = Rc<dyn Fn(&dyn Any)>;
pub(crate) type Listener = Rc<dyn Fn(&Event<'_>)>;
pub(crate) type Listeners = Vec<(ElementId, Vec<Listener>)>;

const STATES: usize = 5;
const EVENTS: usize = 13;

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

#[derive(Clone, Copy)]
pub(crate) enum Interaction {
    Hover,
    Active,
    Focus,
    FocusVisible,
    Disabled,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct FocusSpec {
    pub tab_index: i32,
    pub handle: Option<(u64, Signal<bool>)>,
}

pub(crate) struct TextData {
    pub content: String,
    pub style: TextStyle,
    pub color: Color,
    pub theme: Option<Theme>,
    pub edit: Option<EditState>,
    pub placeholder: String,
    pub placeholder_color: Color,
    layout: Option<TextLayout>,
    inset: Point,
    wrap_width: Option<f32>,
    content_size: Size,
}

impl Default for TextData {
    fn default() -> Self {
        TextData {
            content: String::new(),
            style: TextStyle::default(),
            color: Color::BLACK,
            theme: None,
            edit: None,
            placeholder: String::new(),
            placeholder_color: Color::TRANSPARENT,
            layout: None,
            inset: Point::ZERO,
            wrap_width: None,
            content_size: Size::default(),
        }
    }
}

impl TextData {
    fn shaped(&mut self, system: &mut TextSystem) -> &mut TextLayout {
        self.layout
            .get_or_insert_with(|| system.layout(&self.content, &self.style))
    }
}

#[derive(Clone, Copy)]
struct CachedGlyph {
    mask: AtlasRect,
    left: i32,
    top: i32,
}

pub(crate) struct PaintCache {
    pub atlas: Atlas,
    pub scale: f32,
    glyphs: HashMap<GlyphKey, Option<CachedGlyph>>,
    paths: HashMap<PathKey, Option<(ImageData, i32, i32)>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PathKey {
    path: u64,
    stroke: Option<(u32, bool)>,
    scale: u32,
    offset: (u8, u8),
}

const PATH_CACHE_LIMIT: usize = 1024;
const PATH_BUCKETS: f32 = 4.0;

impl PaintCache {
    pub(crate) fn new(scale: f32) -> Self {
        PaintCache {
            atlas: Atlas::new(),
            scale,
            glyphs: HashMap::new(),
            paths: HashMap::new(),
        }
    }
}

#[derive(Default)]
pub(crate) struct LayerSpec {
    pub commands: Vec<LayerCommand>,
}

#[derive(Default)]
pub(crate) struct ScrollSpec {
    pub observer: Option<Signal<ScrollState>>,
    pub published: Point,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Presence {
    pub enter: Option<(Pose, Timing)>,
    pub exit: Option<(Pose, Timing)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Own,
    Content,
}

type LayerRef = (ElementId, Slot);

struct LayerState {
    id: LayerId,
    dirty: bool,
    frame: Option<Rect>,
    backing: Option<(Color, Corners)>,
    nested: Vec<LayerRef>,
}

#[derive(Default)]
#[expect(clippy::struct_excessive_bools)]
pub(crate) struct ElementSpec {
    pub layer: Option<LayerSpec>,
    pub scroll: Option<ScrollSpec>,
    pub group: Option<String>,
    pub test_id: Option<String>,
    pub text: Option<TextData>,
    pub text_hover: Option<HitListener>,
    pub text_click: Option<HitListener>,
    pub overflow_observer: Option<Signal<f32>>,
    pub style: Style,
    pub variants: [Option<Variant>; STATES],
    pub listeners: [Vec<Listener>; EVENTS],
    pub actions: Vec<(TypeId, ActionHandler)>,
    pub bindings: Vec<(Chord, Rc<dyn Any>)>,
    pub focus: Option<FocusSpec>,
    pub focus_scope: bool,
    pub disabled: bool,
    pub pass_through: bool,
    pub preserve_focus: bool,
    pub root_host: bool,
    pub cursor: Option<Cursor>,
    pub bounds_observer: Option<Signal<Rect>>,
    pub paint_region: Option<Signal<PaintRegion>>,
    pub size_observer: Option<Signal<Size>>,
    pub image: Option<ImageSpec>,
    pub surface: Option<SurfaceSpec>,
    pub canvas: Option<Vec<PaintOp>>,
    pub canvas_measure: Option<CanvasMeasure>,
}

struct Element {
    spec: ElementSpec,
    taffy: NodeId,
    layers: [Option<LayerState>; 2],
    extent: (Size, Size),
    scroll_target: Option<Point>,
    parent: Option<ElementId>,
    children: Vec<ElementId>,
    resolved: Option<Style>,
    states: [bool; STATES],
    bounds: Rect,
    painted: Rect,
    border: Edges,
    followers: Vec<ElementId>,
}

impl Element {
    fn layer(&self, slot: Slot) -> Option<&LayerState> {
        self.layers[slot as usize].as_ref()
    }

    fn disabled(&self) -> bool {
        self.states[Interaction::Disabled as usize]
    }

    fn effective_style(&self) -> &Style {
        self.resolved.as_ref().unwrap_or(&self.spec.style)
    }

    fn resolve(&self) -> Option<Style> {
        let disabled = self.states[Interaction::Disabled as usize];
        let variants = self.spec.variants.iter().zip(self.states).enumerate();
        let live =
            variants.filter(|(index, _)| !disabled || *index == Interaction::Disabled as usize);
        let mut active = live
            .filter_map(|(_, (variant, on))| variant.as_ref().filter(|_| on))
            .peekable();
        active.peek()?;
        Some(active.fold(self.spec.style.clone(), |style, variant| variant(style)))
    }

    fn hidden(&self) -> bool {
        self.effective_style().layout.display == Display::None
    }

    fn unseen(&self) -> bool {
        self.hidden() || self.effective_style().invisible
    }

    fn clips_children(&self) -> bool {
        let overflow = self.effective_style().layout.overflow;
        overflow.x != Overflow::Visible || overflow.y != Overflow::Visible
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaintRegion {
    pub bounds: Rect,
    pub order: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub layouts: u64,
    pub paints: u64,
    pub layer_paints: u64,
    pub passes: u64,
    pub patches: u64,
    pub elements_created: u64,
    pub elements_removed: u64,
    pub layers: usize,
    pub elements: usize,
    pub text: TextStats,
    pub reactive: ReactiveStats,
}

impl Sub for FrameStats {
    type Output = FrameStats;

    fn sub(self, earlier: FrameStats) -> FrameStats {
        FrameStats {
            layouts: self.layouts - earlier.layouts,
            paints: self.paints - earlier.paints,
            layer_paints: self.layer_paints - earlier.layer_paints,
            passes: self.passes - earlier.passes,
            patches: self.patches - earlier.patches,
            elements_created: self.elements_created - earlier.elements_created,
            elements_removed: self.elements_removed - earlier.elements_removed,
            text: self.text - earlier.text,
            reactive: self.reactive - earlier.reactive,
            ..self
        }
    }
}

pub(crate) struct Tree {
    elements: SlotMap<ElementId, Element>,
    taffy: TaffyTree<ElementId>,
    root: ElementId,
    size: Size,
    root_font_size: f32,
    time: f64,
    dirty: Dirty,
    hover_path: Vec<ElementId>,
    press_path: Vec<ElementId>,
    capture: Option<Vec<ElementId>>,
    pub(crate) stats: FrameStats,
    repaint_all: bool,
    commit: Commit,
    next_layer: u64,
    observed: Vec<ElementId>,
    scrolled: Vec<ElementId>,
    motions: HashMap<MotionId, Signal<f32>>,
    focused: Option<ElementId>,
    replaced_focus: Option<(u64, Option<(String, EditState)>)>,
    focus_visible: bool,
    restore: Vec<(ElementId, Option<ElementId>)>,
    focus_fallback: Option<ElementId>,
    handles: HashMap<u64, (ElementId, Signal<bool>)>,
    measured: Vec<ElementId>,
    overlay: Option<ElementId>,
    clipboard: Box<dyn Clipboard>,
    clipboard_target: Rc<()>,
    keymap: Option<Keymap>,
    surfaces: Vec<ElementId>,
    surface_ops: Vec<SurfaceOp>,
    window_visible: bool,
}

impl Tree {
    pub(crate) fn new(size: Size) -> Tree {
        let mut tree = Tree {
            elements: SlotMap::with_key(),
            taffy: TaffyTree::new(),
            root: ElementId::default(),
            size,
            root_font_size: 16.0,
            time: 0.0,
            dirty: Dirty::Layout,
            hover_path: Vec::new(),
            press_path: Vec::new(),
            capture: None,
            stats: FrameStats::default(),
            repaint_all: true,
            commit: Commit::new(),
            next_layer: 0,
            observed: Vec::new(),
            scrolled: Vec::new(),
            motions: HashMap::new(),
            focused: None,
            replaced_focus: None,
            focus_visible: false,
            restore: Vec::new(),
            focus_fallback: None,
            handles: HashMap::new(),
            measured: Vec::new(),
            overlay: None,
            clipboard: Box::new(MemoryClipboard::new()),
            clipboard_target: Rc::new(()),
            keymap: None,
            surfaces: Vec::new(),
            surface_ops: Vec::new(),
            window_visible: true,
        };
        tree.root = tree.create_element(ElementSpec {
            style: root_style(size),
            layer: Some(LayerSpec::default()),
            ..Default::default()
        });
        if let Some(layer) = tree.elements[tree.root].layer(Slot::Own) {
            tree.commit.push(Command::Root(layer.id));
        }
        tree
    }

    pub(crate) fn root(&self) -> ElementId {
        self.root
    }

    pub(crate) fn stats(&self) -> FrameStats {
        let layers = self
            .elements
            .values()
            .flat_map(|e| e.layers.iter().flatten());
        FrameStats {
            layers: layers.count(),
            elements: self.elements.len(),
            ..self.stats
        }
    }

    pub(crate) fn dirty(&self) -> Dirty {
        self.dirty
    }

    pub(crate) fn take_commit(&mut self) -> Option<Commit> {
        (!self.commit.is_empty()).then(|| take(&mut self.commit))
    }

    fn mark(&mut self, dirty: Dirty) {
        self.dirty = self.dirty.max(dirty);
    }

    fn mark_layer(&mut self, id: ElementId, mut inside: bool) {
        let mut current = Some(id);
        while let Some(element) = current.and_then(|id| self.elements.get_mut(id)) {
            let slots = element.layers.iter_mut().rev().skip(usize::from(!inside));
            if let Some(layer) = slots.flatten().next() {
                layer.dirty = true;
                return;
            }
            (current, inside) = (element.parent, true);
        }
    }

    fn mark_element(&mut self, id: ElementId, dirty: Dirty) {
        if dirty.needs_paint() {
            self.mark_layer(id, false);
        }
        self.mark(dirty);
    }

    fn new_layer(&mut self, commands: Vec<LayerCommand>) -> LayerState {
        let id = LayerId(self.next_layer);
        self.next_layer += 1;
        self.commit.push(Command::Create(id));
        for command in commands {
            self.commit.layer(id, command);
        }
        LayerState {
            id,
            dirty: true,
            frame: None,
            backing: None,
            nested: Vec::new(),
        }
    }

    fn sync_backing(&mut self, id: ElementId) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let style = element.effective_style();
        let backing = (
            style.background.unwrap_or(Color::TRANSPARENT),
            style.corner_radii.resolve(self.root_font_size),
        );
        let Some(layer) = &mut element.layers[Slot::Own as usize] else {
            return;
        };
        let previous = layer.backing.replace(backing);
        if previous == Some(backing) {
            return;
        }
        let animate = previous.is_some() && layer.frame.is_some();
        for (prop, value) in Pose::new().background(backing.0).rounded(backing.1).iter() {
            self.commit.layer(
                layer.id,
                LayerCommand::Set {
                    prop,
                    value,
                    animate,
                },
            );
        }
        self.mark(Dirty::Composite);
    }

    pub(crate) fn layer_command(&mut self, id: ElementId, command: LayerCommand) {
        self.slot_command(id, Slot::Own, command);
    }

    pub(crate) fn scroll_to(&mut self, id: ElementId, offset: Point) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        if element.spec.scroll.as_ref().map(|s| s.published) == Some(offset) {
            return;
        }
        element.scroll_target = Some(offset);
        self.scrolled.push(id);
        self.slot_command(id, Slot::Content, LayerCommand::ScrollTo(offset));
    }

    fn slot_command(&mut self, id: ElementId, slot: Slot, command: LayerCommand) {
        if let Some(layer) = self.elements.get(id).and_then(|e| e.layer(slot)) {
            self.scroll_command(layer.id, command);
        }
    }

    pub(crate) fn present(&mut self, id: ElementId, presence: Presence) {
        let Some(element) = self.elements.get(id) else {
            return;
        };
        if presence.enter.is_none() && presence.exit.is_none() {
            return;
        }
        if element.layer(Slot::Own).is_none() {
            self.mark_layer(id, false);
            let layer = self.new_layer(Vec::new());
            self.elements[id].layers[Slot::Own as usize] = Some(layer);
            self.sync_backing(id);
            self.mark(Dirty::Layout);
        }
        if let Some((from, timing)) = presence.enter {
            self.layer_command(id, LayerCommand::Enter { from, timing });
        }
    }

    pub(crate) fn set_root_font_size(&mut self, size: f32) {
        if self.root_font_size == size {
            return;
        }
        self.root_font_size = size;
        let ids: Vec<_> = self.elements.keys().collect();
        for id in ids {
            let element = &self.elements[id];
            if !element.effective_style().rem_lengths.is_empty() {
                self.taffy
                    .set_style(
                        element.taffy,
                        element.effective_style().resolved_layout(size),
                    )
                    .expect("taffy set style");
                self.mark_element(id, Dirty::Layout);
            }
            if self.elements[id].effective_style().corner_radii.rem {
                self.mark_element(id, Dirty::Backing);
                self.sync_backing(id);
            }
        }
        self.refresh_text_subtree(self.root);
    }

    pub(crate) fn create_motion(&mut self, id: MotionId, value: Signal<f32>) {
        if !value.is_alive() {
            return;
        }
        self.commit
            .motion(id, MotionCommand::Create(value.get_untracked()));
        self.motions.insert(id, value);
    }

    pub(crate) fn motion_command(&mut self, id: MotionId, command: MotionCommand) {
        if let MotionCommand::Remove = command {
            self.motions.remove(&id);
        }
        self.commit.motion(id, command);
        self.mark(Dirty::Composite);
    }

    pub(crate) fn publish(&mut self, compositor: &Compositor) {
        for (id, value) in &self.motions {
            if let Some(current) = compositor.motion(*id).filter(|_| value.is_alive()) {
                value.set_if_changed(current);
            }
        }
        for id in &self.measured {
            let observer = self
                .elements
                .get(*id)
                .and_then(|element| element.spec.bounds_observer);
            if let Some(observer) = observer.filter(|observer| observer.is_alive()) {
                observer.set_if_changed(self.window_bounds(compositor, *id));
            }
            if let Some(observer) = self
                .elements
                .get(*id)
                .and_then(|element| element.spec.paint_region)
                .filter(|observer| observer.is_alive())
            {
                observer.set_if_changed(self.paint_region(compositor, *id));
            }
        }
        for id in &self.observed {
            let Some(element) = self.elements.get_mut(*id) else {
                continue;
            };
            if element.scroll_target.is_some() {
                continue;
            }
            let layer = element.layers[Slot::Content as usize].as_ref();
            let state = layer.and_then(|layer| compositor.scroll(layer.id));
            if let (Some(state), Some(scroll)) = (state, element.spec.scroll.as_mut()) {
                scroll.published = state.offset;
                if let Some(observer) = scroll.observer.filter(|observer| observer.is_alive()) {
                    observer.set_if_changed(state);
                }
            }
        }
    }

    pub(crate) fn publish_surfaces(&mut self, compositor: &Compositor, scale: f32) {
        for index in 0..self.surfaces.len() {
            let id = self.surfaces[index];
            let Some(element) = self.elements.get(id) else {
                continue;
            };
            let size = element.bounds.size;
            let shown = self.window_visible
                && !element.unseen()
                && !self.paint_region(compositor, id).bounds.is_empty();
            let placement = Placement {
                width: (size.width * scale).round().max(0.0) as u32,
                height: (size.height * scale).round().max(0.0) as u32,
                scale,
                visible: shown,
            };
            let Some(surface) = self.elements[id].spec.surface.as_mut() else {
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

    pub(crate) fn set_window_visible(&mut self, visible: bool) {
        self.window_visible = visible;
    }

    pub(crate) fn scroll_targets(&self) -> Vec<LayerId> {
        let path = self.hover_path.iter().rev();
        path.filter_map(|id| Some(self.elements.get(*id)?.layer(Slot::Content)?.id))
            .collect()
    }

    pub(crate) fn scroll_command(&mut self, layer: LayerId, command: LayerCommand) {
        self.commit.layer(layer, command);
        self.mark(Dirty::Composite);
    }

    pub(crate) fn set_size(&mut self, size: Size) {
        if self.size != size {
            self.size = size;
            self.update_style(self.root, Dirty::Layout, |s| *s = root_style(size));
        }
    }

    pub(crate) fn invalidate_paint(&mut self) {
        self.repaint_all = true;
        self.mark(Dirty::Paint);
    }

    pub(crate) fn create_element(&mut self, mut spec: ElementSpec) -> ElementId {
        self.stats.elements_created += 1;
        let taffy = self
            .taffy
            .new_leaf(spec.style.resolved_layout(self.root_font_size))
            .expect("taffy node");
        let is_text = spec.text.is_some() || spec.image.is_some() || spec.canvas_measure.is_some();
        let own = spec
            .layer
            .take()
            .map(|layer| self.new_layer(layer.commands));
        let observer = spec.scroll.as_ref().and_then(|s| s.observer);
        let content = spec.scroll.as_ref().map(|_| {
            let source = observer.map(|state| LayerCommand::ScrollSource(scroll_key(state)));
            self.new_layer(source.into_iter().collect())
        });
        let (measured, disabled) = (
            spec.bounds_observer.is_some() || spec.paint_region.is_some(),
            spec.disabled,
        );
        let handle = spec.focus.and_then(|focus| focus.handle);
        let attach = spec
            .surface
            .as_mut()
            .and_then(|surface| surface.attach.take());
        let has_surface = spec.surface.is_some();
        let id = self.elements.insert(Element {
            spec,
            taffy,
            layers: [own, content],
            extent: Default::default(),
            scroll_target: None,
            painted: Rect::ZERO,
            parent: None,
            children: Vec::new(),
            resolved: None,
            states: [false; STATES],
            bounds: Rect::ZERO,
            border: Edges::ZERO,
            followers: Vec::new(),
        });
        if is_text {
            self.taffy
                .set_node_context(taffy, Some(id))
                .expect("taffy context");
        }
        if let Some(state) = observer {
            self.observed.push(id);
            self.scroll_to(id, state.get_untracked().offset);
        }
        if measured {
            self.measured.push(id);
        }
        if let Some((source, status)) = attach {
            self.surface_ops.push(SurfaceOp::Attach(source, status));
        }
        if has_surface {
            self.surfaces.push(id);
        }
        if let Some((handle, focused)) = handle {
            self.handles.insert(handle, (id, focused));
        }
        if disabled {
            self.set_interaction(id, Interaction::Disabled, true);
        }
        self.sync_backing(id);
        id
    }

    pub(crate) fn overlay_root(&mut self) -> ElementId {
        if let Some(overlay) = self
            .overlay
            .filter(|overlay| self.elements.contains_key(*overlay))
        {
            return overlay;
        }
        let mut style = Style::default();
        style.layout.position = Position::Absolute;
        style.layout.inset = Sides::length(0.0);
        let overlay = self.create_element(ElementSpec {
            style,
            pass_through: true,
            layer: Some(LayerSpec::default()),
            ..Default::default()
        });
        let placeholder = self.create_anchor();
        self.attach(overlay, 0, placeholder);
        self.attach(self.root, usize::MAX, overlay);
        self.overlay = Some(overlay);
        overlay
    }

    pub(crate) fn create_anchor(&mut self) -> ElementId {
        let mut style = Style::default();
        style.layout.display = Display::None;
        self.create_element(ElementSpec {
            style,
            ..Default::default()
        })
    }

    pub(crate) fn update_text(
        &mut self,
        id: ElementId,
        dirty: Dirty,
        f: impl FnOnce(&mut TextData),
    ) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let Some(text) = element.spec.text.as_mut() else {
            return;
        };
        let previous = text.content.clone();
        f(text);
        if previous != text.content
            && let Some(edit) = &mut text.edit
        {
            edit.clear_history();
            if self.focused == Some(id) {
                Rc::make_mut(&mut self.clipboard_target);
            }
        }
        if dirty.needs_layout() {
            text.layout = None;
            if let Some(edit) = &mut text.edit {
                edit.stale = true;
            }
            self.taffy.mark_dirty(element.taffy).expect("taffy dirty");
        }
        self.mark_element(id, dirty);
    }

    pub(crate) fn set_image(&mut self, id: ElementId, data: Option<ImageData>) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let Some(image) = element.spec.image.as_mut() else {
            return;
        };
        let resized = image
            .data
            .as_ref()
            .map(|data| (data.width(), data.height()))
            != data.as_ref().map(|data| (data.width(), data.height()));
        image.data = data;
        if resized {
            self.taffy.mark_dirty(element.taffy).expect("taffy dirty");
        }
        self.mark_element(id, if resized { Dirty::Layout } else { Dirty::Paint });
    }

    pub(crate) fn set_canvas(
        &mut self,
        id: ElementId,
        mut ops: Vec<PaintOp>,
        system: &mut TextSystem,
    ) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let mut overflow = 0.0_f32;
        let mut marks = Vec::new();
        for op in &mut ops {
            if let PaintOp::Text(shaped) = op {
                shape_run(shaped, system);
                overflow = overflow.max(shaped.overflow());
                marks.extend(shaped.continuation_marks());
            }
        }
        ops.extend(marks.into_iter().map(|run| {
            let mut shaped = ShapedRun { run, layout: None };
            shape_run(&mut shaped, system);
            PaintOp::Text(shaped)
        }));
        if let Some(observer) = element.spec.overflow_observer.filter(|o| o.is_alive()) {
            observer.set_if_changed(overflow);
        }
        element.spec.canvas = Some(ops);
        let laid_out = element.bounds.size.width;
        let resized = element.spec.canvas_measure.as_mut().is_some_and(|measure| {
            let Some(last) = measure
                .last
                .or((laid_out > 0.0).then(|| Size::new(laid_out, f32::NAN)))
            else {
                return false;
            };
            let height = measure.height(system, last.width);
            measure.last = Some(Size::new(last.width, height));
            height != last.height
        });
        if resized {
            self.taffy.mark_dirty(element.taffy).expect("taffy dirty");
        }
        self.mark_element(id, if resized { Dirty::Layout } else { Dirty::Paint });
    }

    pub(crate) fn set_text_content(&mut self, id: ElementId, content: String) {
        let text = self.elements.get(id).and_then(|e| e.spec.text.as_ref());
        if text.is_some_and(|text| text.content != content) {
            self.update_text(id, Dirty::Layout, |text| text.content = content);
        }
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

    fn refresh_text(&mut self, id: ElementId, inherited: &TextProperties) {
        let Some(element) = self.elements.get(id) else {
            return;
        };
        let properties = inherited.cascade(element.effective_style());
        let children = element.children.clone();
        if let Some(text) = &element.spec.text {
            let default = TextStyle::default();
            let mut style = text.style.clone();
            style.size = properties
                .text_size
                .map_or(default.size, |size| size.resolve(self.root_font_size));
            style.family = properties
                .text_font
                .clone()
                .map_or(default.family, Into::into);
            style.weight = properties.text_weight.unwrap_or(default.weight);
            style.italic = properties.text_italic.unwrap_or(default.italic);
            style.line_height = properties.text_line_height.unwrap_or(default.line_height);
            let color = properties.text_color.unwrap_or_else(|| {
                text.theme
                    .map_or(Color::BLACK, |theme| theme.text.get_untracked())
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

    pub(crate) fn update_style(
        &mut self,
        id: ElementId,
        _dirty: Dirty,
        f: impl FnOnce(&mut Style),
    ) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let old = element.effective_style().clone();
        f(&mut element.spec.style);
        self.apply_style(id, &old);
    }

    fn apply_style(&mut self, id: ElementId, old: &Style) {
        let resolved = self.resolve_style(id);
        let element = &mut self.elements[id];
        let new = resolved.as_ref().unwrap_or(&element.spec.style);
        let text = !TextProperties::same(new, old);
        let dirty = if new.same_box(old) {
            Dirty::None
        } else if new.layout != old.layout || new.rem_lengths != old.rem_lengths {
            self.taffy
                .set_style(element.taffy, new.resolved_layout(self.root_font_size))
                .expect("taffy set style");
            Dirty::Layout
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
        element.resolved = resolved;
        if dirty != Dirty::None {
            self.mark_element(id, dirty);
            self.sync_backing(id);
        }
        if text {
            self.refresh_text_subtree(id);
        }
    }

    fn flat_followers(&self, anchor: ElementId, out: &mut Vec<ElementId>) {
        let Some(element) = self.elements.get(anchor) else {
            return;
        };
        for follower in &element.followers {
            if self.elements.contains_key(*follower) {
                out.push(*follower);
                self.flat_followers(*follower, out);
            }
        }
    }

    pub(crate) fn set_followers(&mut self, anchor: ElementId, followers: Vec<ElementId>) {
        if !self.elements.contains_key(anchor) {
            return;
        }
        let (mut old, mut new) = (Vec::new(), Vec::new());
        self.flat_followers(anchor, &mut old);
        self.elements[anchor].followers = followers;
        self.flat_followers(anchor, &mut new);
        for id in old.iter().filter(|id| !new.contains(id)) {
            self.detach(*id);
        }
        self.place_followers(anchor);
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
        if let (Some((content, old)), Some(text)) = (
            edit,
            self.elements
                .get_mut(new)
                .and_then(|element| element.spec.text.as_mut()),
        ) && let Some(edit) = &mut text.edit
        {
            edit.restore(old, content == text.content);
        }
        self.set_focus(Some(new), self.focus_visible);
    }

    pub(crate) fn clear_followers(&mut self, anchor: ElementId, exit: Option<(Pose, Timing)>) {
        let followers = self
            .elements
            .get_mut(anchor)
            .map(|a| take(&mut a.followers));
        for follower in followers.unwrap_or_default() {
            self.remove(follower, exit);
        }
    }

    fn place_followers(&mut self, anchor: ElementId) {
        let Some(parent) = self.elements.get(anchor).and_then(|a| a.parent) else {
            return;
        };
        let mut desired = Vec::new();
        self.flat_followers(anchor, &mut desired);
        if desired.is_empty() {
            return;
        }
        let children = &self.elements[parent].children;
        let first = 1 + children
            .iter()
            .position(|c| *c == anchor)
            .expect("anchor is a child of its parent");
        for (offset, id) in desired.into_iter().enumerate() {
            if self.elements[parent].children.get(first + offset) != Some(&id) {
                self.detach(id);
                self.attach(parent, first + offset, id);
            }
        }
    }

    pub(crate) fn insert_child(&mut self, parent: ElementId, index: usize, child: ElementId) {
        if self.elements.contains_key(parent) && self.elements.contains_key(child) {
            self.detach(child);
            self.attach(parent, index, child);
            self.place_followers(child);
        }
    }

    pub(crate) fn append_child(&mut self, parent: ElementId, child: ElementId) {
        self.insert_child(parent, usize::MAX, child);
    }

    fn attach(&mut self, parent: ElementId, index: usize, child: ElementId) {
        let child_node = self.elements[child].taffy;
        let last = self
            .overlay
            .filter(|overlay| parent == self.root && *overlay != child);
        let p = &mut self.elements[parent];
        let limit = p.children.len()
            - usize::from(last.is_some_and(|overlay| p.children.contains(&overlay)));
        let index = index.min(limit);
        p.children.insert(index, child);
        self.taffy
            .insert_child_at_index(p.taffy, index, child_node)
            .expect("taffy insert");
        self.elements[child].parent = Some(parent);
        if self.is_connected(parent) {
            self.refresh_variant_subtree(child);
            self.refresh_text_subtree(child);
        }
        self.mark_layer(parent, true);
        self.mark(Dirty::Layout);
    }

    fn is_connected(&self, mut id: ElementId) -> bool {
        loop {
            if id == self.root {
                return true;
            }
            match self.elements.get(id).and_then(|element| element.parent) {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }

    fn detach(&mut self, child: ElementId) {
        let Some(parent) = self.elements.get_mut(child).and_then(|c| c.parent.take()) else {
            return;
        };
        let child_node = self.elements[child].taffy;
        self.mark_layer(parent, true);
        let p = &mut self.elements[parent];
        p.children.retain(|c| *c != child);
        self.taffy
            .remove_child(p.taffy, child_node)
            .expect("taffy remove child");
        self.mark(Dirty::Layout);
    }

    pub(crate) fn remove(&mut self, id: ElementId, exit: Option<(Pose, Timing)>) {
        if id == self.root || !self.elements.contains_key(id) {
            return;
        }
        let mut tops = vec![id];
        self.flat_followers(id, &mut tops);
        if let Some(focused) = self
            .focused
            .filter(|id| self.chain(*id).iter().any(|id| tops.contains(id)))
        {
            let element = &mut self.elements[focused];
            if let Some((handle, _)) = element.spec.focus.and_then(|focus| focus.handle) {
                let state = element
                    .spec
                    .text
                    .as_mut()
                    .and_then(|text| text.edit.take().map(|edit| (text.content.clone(), edit)));
                self.replaced_focus = Some((handle, state));
            }
        }
        for top in &tops {
            self.detach(*top);
        }
        for top in tops {
            let layer = self.elements.get(top).and_then(|e| e.layer(Slot::Own));
            let exits = match (exit, layer) {
                (Some((to, timing)), Some(layer)) => {
                    self.commit
                        .layer(layer.id, LayerCommand::Exit { to, timing });
                    true
                }
                _ => false,
            };
            let mut stack = vec![top];
            while let Some(element) = stack.pop().and_then(|id| self.elements.remove(id)) {
                self.stats.elements_removed += 1;
                self.taffy.remove(element.taffy).expect("taffy remove");
                if let Some(surface) = &element.spec.surface {
                    self.surface_ops.push(SurfaceOp::Detach(surface.id));
                }
                for layer in element.layers.into_iter().flatten().filter(|_| !exits) {
                    self.commit.push(Command::Remove(layer.id));
                }
                stack.extend(element.children);
            }
        }
        let elements = &self.elements;
        for list in [
            &mut self.hover_path,
            &mut self.press_path,
            &mut self.observed,
            &mut self.measured,
            &mut self.surfaces,
        ] {
            list.retain(|e| elements.contains_key(*e));
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|path| path.iter().any(|e| !elements.contains_key(*e)))
        {
            self.capture = None;
        }
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

    fn chain(&self, id: ElementId) -> Vec<ElementId> {
        let mut chain = Vec::new();
        let mut current = Some(id).filter(|id| self.elements.contains_key(*id));
        while let Some(id) = current {
            chain.push(id);
            current = self.elements[id].parent;
        }
        chain.reverse();
        chain
    }

    fn is_disabled(&self, id: ElementId) -> bool {
        self.chain(id)
            .iter()
            .any(|id| self.elements[*id].disabled())
    }

    fn is_focusable(&self, id: ElementId) -> bool {
        let chain = self.chain(id);
        let attached = chain.first() == Some(&self.root);
        let usable = chain.iter().all(|id| {
            let element = &self.elements[*id];
            !element.unseen() && !element.disabled()
        });
        attached && usable && self.elements[id].spec.focus.is_some()
    }

    pub(crate) fn focused(&self) -> Option<ElementId> {
        self.focused
    }

    pub(crate) fn focus_path(&self) -> Vec<ElementId> {
        if let Some(focused) = self.focused {
            self.chain(focused)
        } else {
            let mut path = vec![self.root];
            let mut view = self.elements[self.root].children.first().copied();
            while let Some(id) = view.take() {
                path.push(id);
                let mut children = self.elements[id].children.iter().copied();
                if let Some(host) = children.find(|id| self.elements[*id].spec.root_host) {
                    path.push(host);
                    view = self.elements[host].children.first().copied();
                }
            }
            path
        }
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
                let phase = if on { On::Focus } else { On::Blur };
                let handlers = element.spec.listeners[phase as usize].clone();
                let handle = element
                    .spec
                    .focus
                    .and_then(|focus| focus.handle)
                    .map(|(_, focused)| focused);
                let editable = element
                    .spec
                    .text
                    .as_ref()
                    .is_some_and(|text| text.edit.is_some());
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
            if let Some(edit) = self.elements[id]
                .spec
                .text
                .as_mut()
                .and_then(|text| text.edit.as_mut())
            {
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
            .any(|id| self.elements[*id].spec.preserve_focus)
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

    fn tab_stops(&self) -> Vec<ElementId> {
        let scoped = |id: &ElementId| self.elements[*id].spec.focus_scope;
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
                .spec
                .focus
                .is_some_and(|focus| focus.tab_index >= 0)
        };
        self.collect(scope.unwrap_or(self.root), &mut |id| stop(id), &mut stops);
        stops.sort_by_key(|id| {
            match self.elements[*id]
                .spec
                .focus
                .map_or(0, |focus| focus.tab_index)
            {
                0 => i32::MAX,
                index => index,
            }
        });
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

    pub(crate) fn set_cursor(&mut self, id: ElementId, cursor: Option<Cursor>) {
        if let Some(element) = self.elements.get_mut(id) {
            element.spec.cursor = cursor;
        }
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

    pub(crate) fn listeners(&self, path: &[ElementId], on: On) -> Listeners {
        let pointer = matches!(
            on,
            On::Click | On::PointerDown | On::PointerUp | On::PointerMove
        );
        let disabled = |id: &ElementId| self.elements.get(*id).is_some_and(Element::disabled);
        let enabled = path.iter().position(disabled).filter(|_| pointer);
        let found = path[..enabled.unwrap_or(path.len())].iter().rev();
        found
            .filter_map(|id| {
                Some((
                    *id,
                    self.elements.get(*id)?.spec.listeners[on as usize].clone(),
                ))
            })
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
        for element in path.iter().rev().filter_map(|id| self.elements.get(*id)) {
            for (chord, action) in &element.spec.bindings {
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
            .filter_map(|id| self.elements.get(*id))
            .find_map(|element| {
                let handlers: Vec<_> = element
                    .spec
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
        let elements = || path.iter().rev().filter_map(|id| self.elements.get(*id));
        let action = elements().find_map(|element| {
            let bound = element
                .spec
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
        let mut path = self.pointer_path().iter().rev();
        path.find_map(|id| self.elements.get(*id)?.spec.cursor)
            .unwrap_or_default()
    }

    fn enter(&self, compositor: &Compositor, id: ElementId, slot: Slot) -> Option<Affine> {
        let element = &self.elements[id];
        let pose = compositor.pose(element.layer(slot)?.id)?;
        Some(pose.transform(element.bounds))
    }

    fn transforms(&self, compositor: &Compositor, id: ElementId) -> Vec<Affine> {
        let chain = self.chain(id);
        let steps = chain.iter().enumerate().flat_map(|(index, element)| {
            let own = self.enter(compositor, *element, Slot::Own);
            let content = self
                .enter(compositor, *element, Slot::Content)
                .filter(|_| index + 1 < chain.len());
            [own, content]
        });
        steps.flatten().collect()
    }

    pub(crate) fn bounds_of_all(&self, compositor: &Compositor, name: &str) -> Vec<Rect> {
        let mut bounds = Vec::new();
        for id in self.visible_preorder(self.root) {
            let element = &self.elements[id];
            if element.spec.test_id.as_deref() == Some(name) {
                bounds.push(self.window_bounds(compositor, id));
            }
            for op in element.spec.canvas.iter().flatten() {
                let rect = match op {
                    PaintOp::Region { name: region, rect } if region == name => *rect,
                    PaintOp::Text(run) if run.run.name.as_deref() == Some(name) => run.bounds(),
                    _ => continue,
                };
                let rect = rect.translate(element.bounds.origin);
                bounds.push(self.to_window(compositor, id, rect));
            }
        }
        bounds
    }

    pub(crate) fn text_of(&self, name: &str) -> Option<String> {
        let found = self
            .visible_preorder(self.root)
            .find(|id| self.elements[*id].spec.test_id.as_deref() == Some(name))?;
        Some(
            self.visible_preorder(found)
                .filter_map(|id| self.elements[id].spec.text.as_ref())
                .map(|data| data.content.as_str())
                .collect(),
        )
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

    pub(crate) fn text_click(
        &self,
        compositor: &Compositor,
        path: &[ElementId],
        point: Point,
        modifiers: Modifiers,
    ) {
        batch(|| {
            for id in path.iter().rev() {
                if let Some(callback) = &self.elements[*id].spec.text_click {
                    callback(self.text_hit(compositor, *id, point), modifiers);
                }
            }
        });
    }

    pub(crate) fn text_bounds(&self, compositor: &Compositor, needle: &str) -> Option<Rect> {
        self.visible_preorder(self.root).find_map(|id| {
            let element = &self.elements[id];
            let text = element.spec.text.as_ref()?;
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
            Some(self.to_window(compositor, id, local))
        })
    }

    pub(crate) fn text_hit(
        &self,
        compositor: &Compositor,
        id: ElementId,
        point: Point,
    ) -> Option<TextHit> {
        let spec = &self.elements.get(id)?.spec;
        let local = self.to_local(compositor, id, point);
        if let Some(text) = &spec.text {
            let point = local - text.inset;
            let index = text.layout.as_ref()?.index_at(point.x, point.y)?;
            return Some(TextHit { tag: 0, index });
        }
        spec.canvas.as_ref()?.iter().rev().find_map(|op| match op {
            PaintOp::Text(run) => run.hit(local),
            _ => None,
        })
    }

    pub(crate) fn to_local(&self, compositor: &Compositor, id: ElementId, point: Point) -> Point {
        let Some(element) = self.elements.get(id) else {
            return point;
        };
        let transforms = self.transforms(compositor, id);
        let inverses = transforms.iter().filter_map(Affine::inverse);
        inverses.fold(point, |point, inverse| inverse.apply(point)) - element.bounds.origin
    }

    pub(crate) fn window_bounds(&self, compositor: &Compositor, id: ElementId) -> Rect {
        let Some(element) = self.elements.get(id) else {
            return Rect::ZERO;
        };
        self.to_window(compositor, id, element.bounds)
    }

    fn paint_region(&self, compositor: &Compositor, id: ElementId) -> PaintRegion {
        let chain = self.chain(id);
        let mut bounds = self.window_bounds(compositor, id).intersect(&Rect::new(
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
                bounds = bounds.intersect(&self.to_window(compositor, node, clip));
            }
        }
        PaintRegion { bounds, order }
    }

    fn to_window(&self, compositor: &Compositor, id: ElementId, rect: Rect) -> Rect {
        let transforms = self.transforms(compositor, id);
        transforms
            .iter()
            .rev()
            .fold(rect, |rect, transform| transform.apply_rect(&rect))
    }

    fn set_interaction(&mut self, id: ElementId, which: Interaction, on: bool) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let unchanged = replace(&mut element.states[which as usize], on) == on;
        if unchanged {
            return;
        }
        if element.spec.group.is_some() || matches!(which, Interaction::Disabled) {
            self.refresh_variant_subtree(id);
        } else {
            self.refresh_variant(id);
        }
    }

    fn resolve_style(&self, id: ElementId) -> Option<Style> {
        let element = &self.elements[id];
        let mut resolved = element.resolve();
        if self.is_disabled(id) {
            return resolved;
        }
        for (name, variant) in &element.spec.style.group_styles.0 {
            let mut parent = element.parent;
            let mut active = false;
            while let Some(id) = parent {
                let ancestor = &self.elements[id];
                if ancestor.spec.group.as_ref() == Some(name)
                    && ancestor.states[Interaction::Hover as usize]
                    && !ancestor.disabled()
                {
                    active = true;
                    break;
                }
                parent = ancestor.parent;
            }
            if active {
                resolved = Some(variant(
                    resolved.unwrap_or_else(|| element.spec.style.clone()),
                ));
            }
        }
        resolved
    }

    fn refresh_variant_subtree(&mut self, id: ElementId) {
        self.refresh_variant(id);
        let children = self.elements[id].children.clone();
        for child in children {
            self.refresh_variant_subtree(child);
        }
    }

    fn refresh_variant(&mut self, id: ElementId) {
        let old = self.elements[id].effective_style().clone();
        self.apply_style(id, &old);
    }

    pub(crate) fn update(&mut self, cache: &mut PaintCache, text_system: &mut TextSystem) -> bool {
        if self.dirty == Dirty::None {
            return false;
        }
        if self.dirty.needs_layout() {
            let _layout = trace_span!("layout").entered();
            let root = self.elements[self.root].taffy;
            let elements = &mut self.elements;
            let available = Extent {
                width: AvailableSpace::Definite(self.size.width),
                height: AvailableSpace::Definite(self.size.height),
            };
            self.taffy
                .compute_layout_with_measure(root, available, |inputs, _, context, style| {
                    let spec = context
                        .and_then(|id| elements.get_mut(*id))
                        .map(|e| &mut e.spec);
                    compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |known, available| match spec {
                            Some(ElementSpec {
                                text: Some(text), ..
                            }) => measure_text(text, text_system, known, available),
                            Some(ElementSpec {
                                image: Some(image), ..
                            }) => measure_image(image, known),
                            Some(ElementSpec {
                                canvas_measure: Some(measure),
                                ..
                            }) => measure_canvas(measure, text_system, known, available),
                            _ => Extent::ZERO,
                        },
                    )
                })
                .expect("taffy layout");
            let mut cx = LayoutCx {
                text_system,
                moved: Vec::new(),
            };
            let space = Space {
                origin: Point::ZERO,
                layer: (self.root, Slot::Own),
            };
            batch(|| self.resolve_bounds(self.root, Point::ZERO, space, &mut cx));
            for layer in cx.moved {
                if let Some(layer) = layer_mut(&mut self.elements, layer) {
                    layer.dirty = true;
                }
            }
            self.stats.layouts += 1;
        }
        for id in take(&mut self.scrolled) {
            if let Some(element) = self.elements.get_mut(id) {
                element.scroll_target = None;
            }
        }
        if self.dirty.needs_paint() {
            let _paint = trace_span!("paint").entered();
            let repaint_all = take(&mut self.repaint_all);
            let focused = self.focused;
            let mut cx = PaintCx {
                cache,
                text_system,
                repaint_all,
                layer_paints: 0,
                focused,
                root_font_size: self.root_font_size,
            };
            paint_layers(
                &mut self.elements,
                (self.root, Slot::Own),
                &mut cx,
                &mut self.commit,
            );
            self.stats.layer_paints += cx.layer_paints;
            self.stats.paints += 1;
        }
        self.dirty = Dirty::None;
        true
    }

    fn resolve_bounds(
        &mut self,
        id: ElementId,
        parent_origin: Point,
        mut space: Space,
        cx: &mut LayoutCx<'_>,
    ) {
        let element = &mut self.elements[id];
        let layout = self.taffy.layout(element.taffy).expect("taffy layout");
        let (padding, border) = (layout.padding, layout.border);
        if let Some(text) = element.spec.text.as_mut() {
            let content_width =
                layout.size.width - padding.left - padding.right - border.left - border.right;
            let content_height =
                layout.size.height - padding.top - padding.bottom - border.top - border.bottom;
            text.inset = Point::new(padding.left + border.left, padding.top + border.top);
            text.wrap_width = Some(content_width + WRAP_TOLERANCE);
            text.content_size = Size::new(content_width.max(0.0), content_height.max(0.0));
            let wrap_width = text.wrap_width;
            text.shaped(cx.text_system).break_lines(wrap_width);
            if let (Some(edit), Some(layout)) = (&mut text.edit, &text.layout) {
                edit.selection = edit.current(layout, &text.content);
                edit.stale = false;
                if let Some(signal) = edit.selection_signal.filter(|signal| signal.is_alive()) {
                    signal.set_if_changed(edit.selection.range());
                }
                edit.scroll_to_caret(layout, &text.content, text.content_size);
            }
        }
        let origin = parent_origin + Point::new(layout.location.x, layout.location.y);
        let size = Size::new(layout.size.width, layout.size.height);
        let border = Edges {
            top: border.top,
            right: border.right,
            bottom: border.bottom,
            left: border.left,
        };
        element.bounds = Rect { origin, size };
        if let Some(observer) = element
            .spec
            .size_observer
            .filter(|observer| observer.is_alive())
        {
            observer.set_if_changed(size);
        }
        let within = |space: &Space| Rect {
            origin: origin - space.origin,
            size,
        };
        let mut enter = |layer: &mut Option<LayerState>, slot: Slot, space: &mut Space| {
            let layer = layer.as_mut()?;
            let frame = within(space);
            if layer.frame.replace(frame) != Some(frame) {
                self.commit.layer(layer.id, LayerCommand::Frame(frame));
            }
            *space = Space {
                origin,
                layer: (id, slot),
            };
            Some(layer.id)
        };
        let [own, content] = &mut element.layers;
        enter(own, Slot::Own, &mut space);
        let painted = within(&space);
        if replace(&mut element.painted, painted) != painted || element.border != border {
            cx.moved.push(space.layer);
        }
        element.border = border;
        if let Some(scroller) = enter(content, Slot::Content, &mut space) {
            let viewport = Size::new(
                (size.width - border.left - border.right).max(0.0),
                (size.height - border.top - border.bottom).max(0.0),
            );
            let content = Size::new(
                viewport.width + layout.scroll_width(),
                viewport.height + layout.scroll_height(),
            );
            if replace(&mut element.extent, (viewport, content)) != (viewport, content) {
                let extent = LayerCommand::ScrollExtent { viewport, content };
                self.commit.layer(scroller, extent);
                if let Some(target) = element.scroll_target {
                    self.commit.layer(scroller, LayerCommand::ScrollTo(target));
                }
            }
        }
        for i in 0..element.children.len() {
            let child = self.elements[id].children[i];
            self.resolve_bounds(child, origin, space, cx);
        }
    }

    pub(crate) fn hit_test(&self, compositor: &Compositor, point: Point) -> Vec<ElementId> {
        let mut path = Vec::new();
        self.hit_test_in(compositor, self.root, point, &mut path);
        path
    }

    fn hit_test_in(
        &self,
        compositor: &Compositor,
        id: ElementId,
        point: Point,
        path: &mut Vec<ElementId>,
    ) -> bool {
        let element = &self.elements[id];
        if element.unseen() {
            return false;
        }
        let through = |slot: Slot, point: Point| {
            let layer = element.layer(slot);
            let Some(pose) = layer.and_then(|layer| compositor.pose(layer.id)) else {
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
                .any(|child| self.hit_test_in(compositor, *child, point, path))
        });
        if hit_child || (inside && !element.spec.pass_through) {
            return true;
        }
        path.pop();
        false
    }

    pub(crate) fn set_pointer(
        &mut self,
        compositor: &Compositor,
        point: Option<Point>,
        modifiers: Modifiers,
    ) {
        if self.capture.is_some() {
            return;
        }
        let path = point
            .map(|p| self.hit_test(compositor, p))
            .unwrap_or_default();
        batch(|| {
            for id in self.hover_path.iter().filter(|id| !path.contains(id)) {
                if let Some(callback) = &self.elements[*id].spec.text_hover {
                    callback(None, modifiers);
                }
            }
            if let Some(point) = point {
                for id in &path {
                    if let Some(callback) = &self.elements[*id].spec.text_hover {
                        callback(self.text_hit(compositor, *id, point), modifiers);
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

#[derive(Clone, Copy)]
struct Space {
    origin: Point,
    layer: LayerRef,
}

struct LayoutCx<'a> {
    text_system: &'a mut TextSystem,
    moved: Vec<LayerRef>,
}

struct PaintCx<'a> {
    cache: &'a mut PaintCache,
    text_system: &'a mut TextSystem,
    repaint_all: bool,
    layer_paints: u64,
    focused: Option<ElementId>,
    root_font_size: f32,
}

struct LayerTarget {
    root: LayerRef,
    origin: Point,
    lists: Vec<DrawList>,
    nested: Vec<(LayerRef, Option<Rect>)>,
}

impl LayerTarget {
    fn out(&mut self) -> &mut DrawList {
        if self.lists.len() <= self.nested.len() {
            self.lists.resize_with(self.nested.len() + 1, DrawList::new);
        }
        &mut self.lists[self.nested.len()]
    }

    fn nest(&mut self, layer: LayerRef, clip: Rect) {
        self.nested
            .push((layer, (clip != Rect::EVERYTHING).then_some(clip)));
    }
}

fn layer_ref(elements: &SlotMap<ElementId, Element>, (id, slot): LayerRef) -> Option<&LayerState> {
    elements.get(id)?.layer(slot)
}

fn layer_mut(
    elements: &mut SlotMap<ElementId, Element>,
    (id, slot): LayerRef,
) -> Option<&mut LayerState> {
    elements.get_mut(id)?.layers[slot as usize].as_mut()
}

fn paint_layers(
    elements: &mut SlotMap<ElementId, Element>,
    layer: LayerRef,
    cx: &mut PaintCx<'_>,
    commit: &mut Commit,
) {
    let Some(state) = layer_mut(elements, layer) else {
        return;
    };
    let layer_id = state.id;
    if replace(&mut state.dirty, false) || cx.repaint_all {
        let (id, slot) = layer;
        let element = &elements[id];
        let mut target = LayerTarget {
            root: layer,
            origin: element.bounds.origin,
            lists: Vec::new(),
            nested: Vec::new(),
        };
        match slot {
            Slot::Own => paint(elements, id, Rect::EVERYTHING, &mut target, cx),
            Slot::Content => paint_children(elements, id, Rect::EVERYTHING, &mut target, cx),
        }
        target
            .lists
            .resize_with(target.nested.len() + 1, DrawList::new);
        let nested = target.nested.iter().filter_map(|(nested, clip)| {
            Some(Nested {
                layer: layer_ref(elements, *nested)?.id,
                clip: *clip,
            })
        });
        let content = Content {
            nested: nested.collect(),
            lists: target.lists,
        };
        commit.layer(layer_id, LayerCommand::Content(content));
        cx.layer_paints += 1;
        if let Some(state) = layer_mut(elements, layer) {
            state.nested = target
                .nested
                .into_iter()
                .map(|(nested, _)| nested)
                .collect();
        }
    }
    let nested = |elements: &SlotMap<ElementId, Element>, i: usize| {
        layer_ref(elements, layer).and_then(|state| state.nested.get(i).copied())
    };
    let mut index = 0;
    while let Some(layer) = nested(elements, index) {
        paint_layers(elements, layer, cx, commit);
        index += 1;
    }
}

fn paint(
    elements: &SlotMap<ElementId, Element>,
    id: ElementId,
    clip: Rect,
    target: &mut LayerTarget,
    cx: &mut PaintCx<'_>,
) {
    let element = &elements[id];
    if element.unseen() {
        return;
    }
    let backing = target.root == (id, Slot::Own);
    if element.layer(Slot::Own).is_some() && !backing {
        target.nest((id, Slot::Own), clip);
        return;
    }
    let style = element.effective_style();
    let radii = if backing {
        Corners::ZERO
    } else {
        style.corner_radii.resolve(cx.root_font_size)
    };
    let bounds = Rect {
        origin: element.bounds.origin - target.origin,
        size: element.bounds.size,
    };
    for shadow in &style.shadows {
        target.out().push_shadow(Shadow {
            bounds: bounds.translate(shadow.offset).inflate(shadow.spread),
            clip,
            corner_radii: radii.map(|r| {
                if backing {
                    shadow.spread
                } else {
                    (r + shadow.spread).max(0.0)
                }
            }),
            color: shadow.color,
            blur_radius: shadow.blur,
            backing,
        });
    }

    let has_border = !element.border.is_zero() && style.border_color.a > 0.0;
    let background = style.background.filter(|c| backing || c.a > 0.0);
    if background.is_some() || has_border {
        target.out().push_quad(Quad {
            bounds,
            clip,
            background: background.unwrap_or_default(),
            border_color: style.border_color,
            corner_radii: radii,
            border_widths: if has_border {
                element.border
            } else {
                Edges::ZERO
            },
            backing,
        });
    }

    if let Some(ImageSpec {
        data: Some(data),
        fit,
    }) = &element.spec.image
    {
        let (fitted, source) = fit_image(*fit, bounds, data);
        target.out().push_image(ImageQuad {
            clip: clip.intersect(&bounds),
            corner_radii: radii,
            source,
            ..ImageQuad::new(fitted, data.clone())
        });
    }
    if let Some(ops) = &element.spec.canvas {
        paint_canvas(ops, bounds.origin, clip, target.out(), cx);
    }
    if let Some(text) = &element.spec.text {
        let focused = cx.focused == Some(id);
        paint_text(
            text,
            bounds.origin + text.inset,
            clip,
            focused,
            target.out(),
            cx,
        );
    }

    let border = element.border;
    let padding_box = Rect::new(
        bounds.origin.x + border.left,
        bounds.origin.y + border.top,
        (bounds.size.width - border.left - border.right).max(0.0),
        (bounds.size.height - border.top - border.bottom).max(0.0),
    );
    let child_clip = if element.clips_children() {
        clip.intersect(&padding_box)
    } else {
        clip
    };
    if element.layer(Slot::Content).is_some() {
        target.nest((id, Slot::Content), child_clip);
        return;
    }
    paint_children(elements, id, child_clip, target, cx);
}

fn paint_children(
    elements: &SlotMap<ElementId, Element>,
    id: ElementId,
    clip: Rect,
    target: &mut LayerTarget,
    cx: &mut PaintCx<'_>,
) {
    if !clip.is_empty() {
        for child in &elements[id].children {
            paint(elements, *child, clip, target, cx);
        }
    }
}

fn fit_image(fit: Fit, bounds: Rect, data: &ImageData) -> (Rect, Rect) {
    let natural = Size::new(data.width() as f32, data.height() as f32);
    let full = Rect::new(0.0, 0.0, 1.0, 1.0);
    let ratio = (
        bounds.size.width / natural.width,
        bounds.size.height / natural.height,
    );
    let centered = |size: Size| {
        let origin = Point::new(
            bounds.origin.x + (bounds.size.width - size.width) / 2.0,
            bounds.origin.y + (bounds.size.height - size.height) / 2.0,
        );
        Rect { origin, size }
    };
    match fit {
        Fit::Fill => (bounds, full),
        Fit::None => (centered(natural), full),
        Fit::Contain => {
            let scale = ratio.0.min(ratio.1);
            (
                centered(Size::new(natural.width * scale, natural.height * scale)),
                full,
            )
        }
        Fit::Cover => {
            let scale = ratio.0.max(ratio.1).max(f32::MIN_POSITIVE);
            let (x, y) = (ratio.0 / scale, ratio.1 / scale);
            (bounds, Rect::new((1.0 - x) / 2.0, (1.0 - y) / 2.0, x, y))
        }
    }
}

fn path_mask(
    cache: &mut PaintCache,
    path: &Path,
    stroke: Option<(f32, bool)>,
    origin: Point,
) -> Option<(ImageData, Rect)> {
    let scale = cache.scale;
    let device = (origin.x * scale, origin.y * scale);
    let base = (device.0.floor(), device.1.floor());
    let bucket = |value: f32, base: f32| {
        ((value - base) * PATH_BUCKETS)
            .floor()
            .min(PATH_BUCKETS - 1.0)
    };
    let offset = (bucket(device.0, base.0), bucket(device.1, base.1));
    let key = PathKey {
        path: path.id(),
        stroke: stroke.map(|(width, round)| (width.to_bits(), round)),
        scale: scale.to_bits(),
        offset: (offset.0 as u8, offset.1 as u8),
    };
    if cache.paths.len() >= PATH_CACHE_LIMIT && !cache.paths.contains_key(&key) {
        cache.paths.clear();
    }
    let rendered = cache.paths.entry(key).or_insert_with(|| {
        let shift = (offset.0 / PATH_BUCKETS, offset.1 / PATH_BUCKETS);
        let transform = Transform::scale(scale, scale).then_translate(shift.0, shift.1);
        let mut mask = Mask::new(path.commands());
        mask.transform(Some(transform));
        match stroke {
            Some((width, round)) => {
                let mut stroke = Stroke::new(width);
                if round {
                    stroke.cap(Cap::Round).join(Join::Round);
                }
                mask.style(stroke)
            }
            None => mask.style(Fill::NonZero),
        };
        let (pixels, placement) = mask.render();
        let image = ImageData::alpha8(placement.width, placement.height, pixels).ok()?;
        Some((image, placement.left, placement.top))
    });
    let (image, left, top) = rendered.as_ref()?;
    let bounds = Rect::new(
        (base.0 + *left as f32) / scale,
        (base.1 + *top as f32) / scale,
        image.width() as f32 / scale,
        image.height() as f32 / scale,
    );
    Some((image.clone(), bounds))
}

fn paint_canvas(
    ops: &[PaintOp],
    origin: Point,
    clip: Rect,
    out: &mut DrawList,
    cx: &mut PaintCx<'_>,
) {
    for op in ops {
        match op {
            PaintOp::Quad(quad) => {
                let clip = clip.intersect(&quad.clip.translate(origin));
                out.push_quad(Quad {
                    bounds: quad.bounds.translate(origin),
                    clip,
                    ..*quad
                });
            }
            PaintOp::Image(image) => {
                let clip = clip.intersect(&image.clip.translate(origin));
                out.push_image(ImageQuad {
                    bounds: image.bounds.translate(origin),
                    clip,
                    ..image.clone()
                });
            }
            PaintOp::Path {
                path,
                stroke,
                color,
            } => {
                if let Some((image, bounds)) = path_mask(cx.cache, path, *stroke, origin) {
                    out.push_image(ImageQuad {
                        clip,
                        tint: *color,
                        ..ImageQuad::new(bounds, image)
                    });
                }
            }
            PaintOp::Text(ShapedRun {
                run,
                layout: Some(layout),
            }) => {
                let clip = run
                    .clip
                    .map_or(clip, |own| clip.intersect(&own.translate(origin)));
                paint_glyphs(layout, origin + run.origin, clip, run.color, out, cx);
            }
            PaintOp::Text(_) | PaintOp::Region { .. } => {}
        }
    }
}

fn paint_text(
    text: &TextData,
    mut origin: Point,
    mut clip: Rect,
    focused: bool,
    out: &mut DrawList,
    cx: &mut PaintCx<'_>,
) {
    let Some(layout) = text.layout.as_ref() else {
        return;
    };
    let Some(edit) = &text.edit else {
        return paint_glyphs(layout, origin, clip, text.color, out, cx);
    };
    if text.content.is_empty() && !text.placeholder.is_empty() {
        let mut shaped = cx.text_system.layout(&text.placeholder, &text.style);
        shaped.break_lines(text.wrap_width);
        paint_glyphs(&shaped, origin, clip, text.placeholder_color, out, cx);
    }
    clip = clip.intersect(
        &Rect {
            origin,
            size: text.content_size,
        }
        .inflate(CARET_WIDTH),
    );
    origin = origin - edit.scroll;
    let rect = |rect: &TextRect| {
        Rect::new(
            origin.x + rect.x,
            origin.y + rect.y,
            rect.width,
            rect.height,
        )
    };
    let selection = edit.current(layout, &text.content);
    if focused {
        for selected in &layout.selection_rects(selection) {
            out.push_quad(Quad {
                clip,
                ..Quad::new(rect(selected), edit.selection_color)
            });
        }
    }
    paint_glyphs(layout, origin, clip, text.color, out, cx);
    if !focused {
        return;
    }
    for composed in edit
        .compose(layout, &text.content)
        .iter()
        .flat_map(|range| layout.selection_rects(*range))
    {
        let line = rect(&composed);
        let underline = Rect::new(line.origin.x, line.max().y - 1.5, line.size.width, 1.0);
        out.push_quad(Quad {
            clip,
            ..Quad::new(underline, text.color)
        });
    }
    if selection.is_collapsed() && edit.caret_visible {
        let caret = rect(&layout.caret(selection, CARET_WIDTH));
        out.push_quad(Quad {
            clip,
            ..Quad::new(caret, edit.caret_color.unwrap_or(text.color))
        });
    }
}

fn paint_glyphs(
    layout: &TextLayout,
    origin: Point,
    clip: Rect,
    color: Color,
    out: &mut DrawList,
    cx: &mut PaintCx<'_>,
) {
    let paint_rect = |out: &mut DrawList, rect: TextRect, override_color: Option<Color>| {
        out.push_quad(Quad {
            clip,
            ..Quad::new(
                Rect::new(
                    origin.x + rect.x,
                    origin.y + rect.y,
                    rect.width,
                    rect.height,
                ),
                override_color.unwrap_or(color),
            )
        });
    };
    layout.for_each_rect(false, |rect, color| paint_rect(out, rect, color));
    let PaintCache {
        atlas,
        glyphs,
        scale,
        ..
    } = &mut *cx.cache;
    let scale = *scale;
    layout.for_each_glyph(|glyph| {
        let color = glyph.style.color.unwrap_or(color);
        if color.a <= 0.0 {
            return;
        }
        let (key, pen_x, pen_y) = glyph.snap((origin.x, origin.y), scale);
        let cached = *glyphs.entry(key).or_insert_with(|| {
            let image = cx.text_system.rasterize(&glyph, &key)?;
            let mask = atlas.insert(image.width, image.height, &image.data)?;
            Some(CachedGlyph {
                mask,
                left: image.left,
                top: image.top,
            })
        });
        let Some(cached) = cached else { return };
        let origin = Point::new(
            (pen_x + cached.left) as f32 / scale,
            (pen_y - cached.top) as f32 / scale,
        );
        out.push_glyph(Glyph {
            origin,
            clip,
            color,
            mask: cached.mask,
        });
    });
    layout.for_each_rect(true, |rect, color| paint_rect(out, rect, color));
}

fn measure_text(
    text: &mut TextData,
    system: &mut TextSystem,
    known: Extent<Option<f32>>,
    available: Extent<AvailableSpace>,
) -> Extent<f32> {
    let layout = text.shaped(system);
    let constraint = known.width.or(match available.width {
        AvailableSpace::Definite(width) => Some(width),
        AvailableSpace::MinContent => Some(layout.content_widths().0),
        AvailableSpace::MaxContent => None,
    });
    layout.break_lines(constraint);
    let (width, mut height) = layout.size();
    if let Some(edit) = &text.edit {
        let row = text.style.size * text.style.line_height;
        height = height.clamp(
            row * edit.min_rows as f32,
            row * edit.max_rows.max(edit.min_rows) as f32,
        );
    }
    Extent {
        width: known.width.unwrap_or(width.ceil()),
        height: known.height.unwrap_or(height.ceil()),
    }
}

fn shape_run(shaped: &mut ShapedRun, system: &mut TextSystem) {
    let mut layout = system.layout(&shaped.run.content, &shaped.run.style);
    layout.break_lines(shaped.run.wrap);
    if shaped.run.right_aligned {
        shaped.run.origin.x -= layout.size().0;
        shaped.run.right_aligned = false;
    }
    shaped.layout = Some(Box::new(layout));
}

fn measure_canvas(
    measure: &mut CanvasMeasure,
    system: &mut TextSystem,
    known: Extent<Option<f32>>,
    available: Extent<AvailableSpace>,
) -> Extent<f32> {
    let width = known.width.or(match available.width {
        AvailableSpace::Definite(width) => Some(width),
        AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
    });
    let height = known
        .height
        .unwrap_or_else(|| measure.height(system, width.unwrap_or(f32::INFINITY)));
    if let (Some(width), None) = (width, known.height) {
        measure.last = Some(Size::new(width, height));
    }
    Extent {
        width: known.width.unwrap_or(0.0),
        height,
    }
}

fn measure_image(image: &ImageSpec, known: Extent<Option<f32>>) -> Extent<f32> {
    let natural = image.data.as_ref().map_or((0.0, 0.0), |data| {
        (data.width() as f32, data.height() as f32)
    });
    let ratio = if natural.1 > 0.0 {
        natural.0 / natural.1
    } else {
        1.0
    };
    let (width, height) = match (known.width, known.height) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) => (width, width / ratio),
        (None, Some(height)) => (height * ratio, height),
        (None, None) => natural,
    };
    Extent { width, height }
}

fn root_style(size: Size) -> Style {
    let mut style = Style::default();
    style.layout.size = Extent {
        width: Dimension::length(size.width),
        height: Dimension::length(size.height),
    };
    style
}
