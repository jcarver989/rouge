use crate::animation::{Pose, Timing};
use crate::canvas::{CanvasData, HitListener};
use crate::clipboard::{Clipboard, MemoryClipboard};
use crate::compositor::{Compositor, MotionId};
use crate::event::{Chord, Cursor};
use crate::image::ImageSpec;
use crate::keymap::Keymap;
use crate::scroll::scroll_key;
use crate::style::{Declaration, Dirty, Style, base_layout};
use crate::surface::{SurfaceOp, SurfaceSpec};
use crate::text::TextStats;
use crate::view::Queue;
use events::Listener;
use focus::ReplacedFocus;
use fragment::Fragment;
use layers::{LayerState, Slot};
use layout::root_layout;
use rouge_reactive::{ReactiveStats, Signal};
use rouge_render::{Edges, Rect, Size};
use slotmap::{SlotMap, new_key_type};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::Sub;
use std::rc::Rc;
use taffy::{NodeId, Position, Rect as Sides, Style as Layout, TaffyTree};

mod cascade;
mod content;
pub(crate) mod edit;
mod events;
mod focus;
mod fragment;
mod geometry;
mod layers;
mod layout;
mod paint;
mod scroll;
mod surfaces;

pub(crate) use content::TextData;
pub(crate) use events::{ActionHandler, Event, Listeners, On, emit};
pub(crate) use focus::FocusSpec;
pub(crate) use fragment::{FragmentId, Node};
pub use geometry::PaintRegion;
pub(crate) use layers::{LayerSpec, Presence};
pub(crate) use paint::PaintCache;
pub(crate) use scroll::ScrollSpec;

new_key_type! {
    pub(crate) struct ElementId;
}

pub(crate) const STATES: usize = 5;

pub(crate) type Exit = Option<(Pose, Timing)>;

#[derive(Clone, Copy)]
pub(crate) enum Interaction {
    Hover,
    Active,
    Focus,
    FocusVisible,
    Disabled,
}

#[derive(Default)]
pub(crate) enum Kind {
    #[default]
    Div,
    Text(Box<TextData>),
    Image(Box<ImageSpec>),
    Canvas(Box<CanvasData>),
    Surface(Box<SurfaceSpec>),
}

impl Kind {
    pub(crate) fn text(&self) -> Option<&TextData> {
        match self {
            Kind::Text(text) => Some(text),
            _ => None,
        }
    }

    pub(crate) fn text_mut(&mut self) -> Option<&mut TextData> {
        match self {
            Kind::Text(text) => Some(text),
            _ => None,
        }
    }

    fn canvas(&self) -> Option<&CanvasData> {
        match self {
            Kind::Canvas(canvas) => Some(canvas),
            _ => None,
        }
    }

    fn measured(&self) -> bool {
        match self {
            Kind::Text(_) | Kind::Image(_) => true,
            Kind::Canvas(canvas) => canvas.measure.is_some(),
            Kind::Div | Kind::Surface(_) => false,
        }
    }
}

#[derive(Default)]
pub(crate) struct Extras {
    pub variants: [Vec<Declaration>; STATES],
    pub groups: Vec<(String, Vec<Declaration>)>,
    pub layout: Option<Box<Layout>>,
    pub listeners: Vec<(On, Listener)>,
    pub actions: Vec<(TypeId, ActionHandler)>,
    pub bindings: Vec<(Chord, Rc<dyn Any>)>,
    pub focus: Option<FocusSpec>,
    pub scroll: Option<ScrollSpec>,
    pub group: Option<String>,
    pub test_id: Option<String>,
    pub text_hover: Option<HitListener>,
    pub text_click: Option<HitListener>,
    pub size_observer: Option<Signal<Size>>,
    pub bounds_observer: Option<Signal<Rect>>,
    pub paint_region: Option<Signal<PaintRegion>>,
}

#[derive(Clone, Copy, Default)]
#[expect(clippy::struct_excessive_bools)]
pub(crate) struct Flags {
    pub pass_through: bool,
    pub preserve_focus: bool,
    pub root_host: bool,
    pub focus_scope: bool,
}

pub(crate) struct ElementSpec {
    pub style: Style,
    pub layout: Layout,
    pub kind: Kind,
    pub extras: Option<Box<Extras>>,
    pub layer: Option<LayerSpec>,
    pub cursor: Option<Cursor>,
    pub flags: Flags,
    pub disabled: bool,
}

impl Default for ElementSpec {
    fn default() -> Self {
        ElementSpec {
            style: Style::default(),
            layout: base_layout(),
            kind: Kind::default(),
            extras: None,
            layer: None,
            cursor: None,
            flags: Flags::default(),
            disabled: false,
        }
    }
}

impl ElementSpec {
    pub(crate) fn extras(&mut self) -> &mut Extras {
        self.extras.get_or_insert_default()
    }

    pub(crate) fn declare(&mut self, declaration: &Declaration) {
        self.style.apply(declaration);
        declaration.lay_out(&mut self.layout);
    }
}

impl Extras {
    fn variant(&self, slot: usize) -> Option<&Vec<Declaration>> {
        match slot.checked_sub(STATES) {
            None => self.variants.get(slot),
            Some(group) => self.groups.get(group).map(|(_, variant)| variant),
        }
    }

    pub(crate) fn variant_mut(&mut self, slot: usize) -> Option<&mut Vec<Declaration>> {
        match slot.checked_sub(STATES) {
            None => self.variants.get_mut(slot),
            Some(group) => self.groups.get_mut(group).map(|(_, variant)| variant),
        }
    }

    fn lays_out(&self) -> bool {
        let groups = self.groups.iter().map(|(_, variant)| variant);
        self.variants
            .iter()
            .chain(groups)
            .flatten()
            .any(Declaration::is_layout)
    }
}

struct Element {
    style: Style,
    resolved: Option<Box<Style>>,
    kind: Kind,
    extras: Option<Box<Extras>>,
    layers: [Option<Box<LayerState>>; 2],
    taffy: NodeId,
    parent: Option<ElementId>,
    fragment: Option<FragmentId>,
    children: Vec<ElementId>,
    bounds: Rect,
    painted: Rect,
    border: Edges,
    states: [bool; STATES],
    cursor: Option<Cursor>,
    flags: Flags,
}

impl Element {
    fn extras(&self) -> Option<&Extras> {
        self.extras.as_deref()
    }

    fn layer(&self, slot: Slot) -> Option<&LayerState> {
        self.layers[slot as usize].as_deref()
    }

    fn disabled(&self) -> bool {
        self.states[Interaction::Disabled as usize]
    }

    fn effective_style(&self) -> &Style {
        self.resolved.as_deref().unwrap_or(&self.style)
    }

    fn hidden(&self) -> bool {
        self.effective_style().hidden
    }

    fn unseen(&self) -> bool {
        self.hidden() || self.effective_style().invisible
    }

    fn clips_children(&self) -> bool {
        self.effective_style().clips
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub layouts: u64,
    pub paints: u64,
    pub layer_paints: u64,
    pub relayouts: u64,
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
            relayouts: self.relayouts - earlier.relayouts,
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
    fragments: SlotMap<FragmentId, Fragment>,
    taffy: TaffyTree<ElementId>,
    root: ElementId,
    content: FragmentId,
    size: Size,
    time: f64,
    dirty: Dirty,
    hover_path: Vec<ElementId>,
    press_path: Vec<ElementId>,
    capture: Option<Vec<ElementId>>,
    pub(crate) stats: FrameStats,
    pub(crate) compositor: Compositor,
    repaint_all: bool,
    next_layer: u64,
    observers: Vec<ElementId>,
    scrolled: Vec<ElementId>,
    motions: HashMap<MotionId, Signal<f32>>,
    focused: Option<ElementId>,
    replaced_focus: Option<ReplacedFocus>,
    focus_visible: bool,
    restore: Vec<(ElementId, Option<ElementId>)>,
    focus_fallback: Option<ElementId>,
    handles: HashMap<u64, (ElementId, Signal<bool>)>,
    overlay: Option<ElementId>,
    clipboard: Box<dyn Clipboard>,
    clipboard_target: Rc<()>,
    queue: Queue,
    keymap: Option<Keymap>,
    surfaces: Vec<ElementId>,
    surface_ops: Vec<SurfaceOp>,
    window_visible: bool,
}

impl Tree {
    pub(crate) fn new(size: Size, queue: Queue) -> Tree {
        let mut tree = Tree {
            elements: SlotMap::with_key(),
            fragments: SlotMap::with_key(),
            taffy: TaffyTree::new(),
            root: ElementId::default(),
            content: FragmentId::default(),
            size,
            time: 0.0,
            dirty: Dirty::Layout,
            hover_path: Vec::new(),
            press_path: Vec::new(),
            capture: None,
            stats: FrameStats::default(),
            compositor: Compositor::new(),
            repaint_all: true,
            next_layer: 0,
            observers: Vec::new(),
            scrolled: Vec::new(),
            motions: HashMap::new(),
            focused: None,
            replaced_focus: None,
            focus_visible: false,
            restore: Vec::new(),
            focus_fallback: None,
            handles: HashMap::new(),
            overlay: None,
            clipboard: Box::new(MemoryClipboard::new()),
            clipboard_target: Rc::new(()),
            queue,
            keymap: None,
            surfaces: Vec::new(),
            surface_ops: Vec::new(),
            window_visible: true,
        };
        tree.root = tree.create_element(ElementSpec {
            layout: root_layout(size),
            layer: Some(LayerSpec::default()),
            ..Default::default()
        });
        if let Some(layer) = tree.elements[tree.root].layer(Slot::Own) {
            tree.compositor.set_root(layer.id);
        }
        tree.content = tree.fragment(Node::Element(tree.root));
        tree
    }

    pub(crate) fn content(&self) -> Node {
        Node::Fragment(self.content)
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

    pub(crate) fn set_size(&mut self, size: Size) {
        if self.size != size {
            self.size = size;
            let node = self.elements[self.root].taffy;
            self.taffy
                .set_style(node, root_layout(size))
                .expect("taffy set style");
            self.mark_element(self.root, Dirty::Layout);
        }
    }

    pub(crate) fn invalidate_paint(&mut self) {
        self.repaint_all = true;
        self.mark(Dirty::Paint);
    }

    pub(crate) fn create_element(&mut self, mut spec: ElementSpec) -> ElementId {
        self.stats.elements_created += 1;
        if let Some(extras) = spec
            .extras
            .as_deref_mut()
            .filter(|extras| extras.lays_out())
        {
            extras.layout = Some(Box::new(spec.layout.clone()));
        }
        let taffy = self.taffy.new_leaf(spec.layout).expect("taffy node");
        let own = spec.layer.take().map(|layer| self.new_layer(layer));
        let extras = spec.extras.as_deref();
        let observer = extras
            .and_then(|extras| extras.scroll.as_ref())
            .map(|scroll| scroll.observer);
        let content = observer.map(|observer| {
            let layer = self.new_layer(LayerSpec::default());
            if let Some(state) = observer {
                self.compositor
                    .set_scroll_source(layer.id, scroll_key(state));
            }
            layer
        });
        let observer = observer.flatten();
        let selection = spec.kind.text().and_then(|text| text.edit.as_ref());
        let watched = observer.is_some()
            || extras.is_some_and(|extras| {
                extras.size_observer.is_some()
                    || extras.bounds_observer.is_some()
                    || extras.paint_region.is_some()
            })
            || selection.is_some_and(|edit| edit.selection_signal.is_some());
        let handle = extras
            .and_then(|extras| extras.focus)
            .and_then(|focus| focus.handle);
        let attach = match &mut spec.kind {
            Kind::Surface(surface) => Some(surface.attach.take()),
            _ => None,
        };
        let measured = spec.kind.measured();
        let id = self.elements.insert(Element {
            style: spec.style,
            resolved: None,
            kind: spec.kind,
            extras: spec.extras,
            layers: [own.map(Box::new), content.map(Box::new)],
            taffy,
            parent: None,
            fragment: None,
            children: Vec::new(),
            bounds: Rect::ZERO,
            painted: Rect::ZERO,
            border: Edges::ZERO,
            states: [false; STATES],
            cursor: spec.cursor,
            flags: spec.flags,
        });
        if measured {
            self.taffy
                .set_node_context(taffy, Some(id))
                .expect("taffy context");
        }
        if watched {
            self.observers.push(id);
        }
        if let Some(state) = observer {
            self.scroll_to(id, state.get_untracked().offset);
        }
        if let Some(attach) = attach {
            if let Some((source, status)) = attach {
                self.surface_ops.push(SurfaceOp::Attach(source, status));
            }
            self.surfaces.push(id);
        }
        if let Some((handle, focused)) = handle {
            self.handles.insert(handle, (id, focused));
        }
        if spec.disabled {
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
        let overlay = self.create_element(ElementSpec {
            layout: Layout {
                position: Position::Absolute,
                inset: Sides::length(0.0),
                ..base_layout()
            },
            layer: Some(LayerSpec::default()),
            flags: Flags {
                pass_through: true,
                ..Flags::default()
            },
            ..Default::default()
        });
        self.append(Node::Element(self.root), overlay);
        self.overlay = Some(overlay);
        overlay
    }

    pub(crate) fn set_cursor(&mut self, id: ElementId, cursor: Option<Cursor>) {
        if let Some(element) = self.elements.get_mut(id) {
            element.cursor = cursor;
        }
    }

    fn attach(&mut self, parent: ElementId, index: usize, child: ElementId) {
        let child_node = self.elements[child].taffy;
        let p = &mut self.elements[parent];
        let index = index.min(p.children.len());
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
}
