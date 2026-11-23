use crate::access::Semantics;
use crate::animation::{Pose, Timing};
use crate::canvas::{CanvasData, HitListener};
use crate::clipboard::{Clipboard, MemoryClipboard};
use crate::compositor::{Layers, MotionId, SurfaceOp};
use crate::event::{Chord, Cursor, DragAxis};
use crate::image::ImageSpec;
use crate::keymap::Keymap;
use crate::scroll::scroll_key;
use crate::style::{Declaration, Dirty, Style, base_layout};
use crate::surface::{StatusSink, SurfaceSpec};
use crate::text::TextStats;
use crate::theme::{Palette, Theme};
use crate::view::Queue;
use access::Access;
use events::Listener;
use focus::ReplacedFocus;
use fragment::Fragment;
use layers::{LayerState, Slot};
use layout::{LayoutNode, root_layout};
use rouge_reactive::{ReactiveStats, Signal};
use rouge_render::{Color, Edges, Rect, Size, SurfaceId};
use rouge_theme::UiPalette;
use rustc_hash::FxHashMap;
use slotmap::{SecondaryMap, SlotMap, new_key_type};
use std::any::{Any, TypeId};
use std::mem::replace;
use std::ops::Sub;
use std::rc::Rc;
use taffy::{Position, Rect as Sides, Style as Layout};

mod access;
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
mod selection;
mod surfaces;

pub(crate) use content::TextData;
pub(crate) use events::{ActionHandler, Event, Listeners, On, emit};
pub(crate) use focus::FocusSpec;
pub(crate) use fragment::{FragmentId, Node};
pub use geometry::PaintRegion;
pub(crate) use layers::{LayerSpec, Presence};
pub(crate) use paint::PaintCache;
pub(crate) use scroll::ScrollSpec;
use selection::SelectionDrag;
pub(crate) use selection::{RowSource, SelectionScope};

new_key_type! {
    pub(crate) struct ElementId;
}

pub(crate) const STATES: usize = 7;

fn theme_at(elements: &SlotMap<ElementId, Element>, id: ElementId) -> Option<Theme> {
    let mut current = Some(id);
    while let Some(element) = current.and_then(|id| elements.get(id)) {
        if let Some(theme) = element.extras().and_then(|extras| extras.theme) {
            return Some(theme);
        }
        current = element.parent;
    }
    None
}

pub(crate) type Exit = Option<(Pose, Timing)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interaction {
    Hover,
    Active,
    Selected,
    Checked,
    Focus,
    FocusVisible,
    Disabled,
}

impl Interaction {
    pub(crate) const ALL: [Interaction; STATES] = [
        Interaction::Hover,
        Interaction::Active,
        Interaction::Selected,
        Interaction::Checked,
        Interaction::Focus,
        Interaction::FocusVisible,
        Interaction::Disabled,
    ];

    pub(crate) fn pointer(self) -> bool {
        matches!(
            self,
            Interaction::Hover
                | Interaction::Active
                | Interaction::Focus
                | Interaction::FocusVisible
        )
    }
}

pub(crate) enum Trigger {
    GroupHover(String),
    When(bool),
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
}

#[derive(Default)]
pub(crate) struct Extras {
    pub variants: [Vec<Declaration>; STATES],
    pub triggers: Vec<(Trigger, Vec<Declaration>)>,
    pub layout: Option<Box<Layout>>,
    pub listeners: Vec<(On, Listener)>,
    pub actions: Vec<(TypeId, ActionHandler)>,
    pub bindings: Vec<(Chord, Rc<dyn Any>)>,
    pub focus: Option<FocusSpec>,
    pub scroll: Option<ScrollSpec>,
    pub group: Option<String>,
    pub test_id: Option<String>,
    pub test_scope: bool,
    pub text_hover: Option<HitListener>,
    pub text_click: Option<HitListener>,
    pub theme: Option<Theme>,
    pub size_observer: Option<Signal<Size>>,
    pub bounds_observer: Option<Signal<Rect>>,
    pub paint_region: Option<Signal<PaintRegion>>,
    pub selection: Option<Box<SelectionScope>>,
    pub select_row: Option<usize>,
    pub row_source: Option<RowSource>,
    pub access: Option<Box<Semantics>>,
}

#[derive(Clone, Copy, Default)]
#[expect(clippy::struct_excessive_bools)]
pub(crate) struct Flags {
    pub pass_through: bool,
    pub preserve_focus: bool,
    pub root_host: bool,
    pub focus_scope: bool,
    pub unselectable: bool,
    pub drag_axis: Option<DragAxis>,
}

pub(crate) struct ElementSpec {
    pub layout: Layout,
    pub kind: Kind,
    pub extras: Option<Box<Extras>>,
    pub layer: Option<LayerSpec>,
    pub flags: Flags,
}

impl Default for ElementSpec {
    fn default() -> Self {
        ElementSpec {
            layout: base_layout(),
            kind: Kind::default(),
            extras: None,
            layer: None,
            flags: Flags::default(),
        }
    }
}

impl ElementSpec {
    pub(crate) fn extras(&mut self) -> &mut Extras {
        self.extras.get_or_insert_default()
    }
}

struct PendingStyle {
    old: Style,
    relayout: bool,
}

impl Extras {
    fn variant(&self, slot: usize) -> Option<&Vec<Declaration>> {
        match slot.checked_sub(STATES) {
            None => self.variants.get(slot),
            Some(trigger) => self.triggers.get(trigger).map(|(_, variant)| variant),
        }
    }

    pub(crate) fn variant_mut(&mut self, slot: usize) -> Option<&mut Vec<Declaration>> {
        match slot.checked_sub(STATES) {
            None => self.variants.get_mut(slot),
            Some(trigger) => self.triggers.get_mut(trigger).map(|(_, variant)| variant),
        }
    }

    fn lays_out(&self) -> bool {
        let triggers = self.triggers.iter().map(|(_, variant)| variant);
        self.variants
            .iter()
            .chain(triggers)
            .flatten()
            .any(Declaration::is_layout)
    }
}

struct Element {
    style: Style,
    resolved: Option<Box<Style>>,
    pending: Option<Box<PendingStyle>>,
    kind: Kind,
    extras: Option<Box<Extras>>,
    layers: [Option<Box<LayerState>>; 2],
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
    pub style_resolutions: u64,
    pub elements_created: u64,
    pub elements_removed: u64,
    pub access_nodes: u64,
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
            style_resolutions: self.style_resolutions - earlier.style_resolutions,
            elements_created: self.elements_created - earlier.elements_created,
            elements_removed: self.elements_removed - earlier.elements_removed,
            access_nodes: self.access_nodes - earlier.access_nodes,
            text: self.text - earlier.text,
            reactive: self.reactive - earlier.reactive,
            ..self
        }
    }
}

pub(crate) struct Tree {
    elements: SlotMap<ElementId, Element>,
    fragments: SlotMap<FragmentId, Fragment>,
    layouts: SecondaryMap<ElementId, LayoutNode>,
    root: ElementId,
    content: FragmentId,
    size: Size,
    time: f64,
    dirty: Dirty,
    hover_path: Vec<ElementId>,
    press_path: Vec<ElementId>,
    capture: Option<Vec<ElementId>>,
    pub(crate) stats: FrameStats,
    pub(crate) layers: Layers,
    repaint_all: bool,
    observers: Vec<ElementId>,
    dirty_styles: Vec<ElementId>,
    themes: Vec<(Theme, Palette)>,
    scrolled: Vec<ElementId>,
    motions: FxHashMap<MotionId, Signal<f32>>,
    focused: Option<ElementId>,
    replaced_focus: Option<ReplacedFocus>,
    focus_visible: bool,
    restore: Vec<(ElementId, Option<ElementId>)>,
    focus_fallback: Option<ElementId>,
    handles: FxHashMap<u64, (ElementId, Signal<bool>)>,
    overlay: Option<ElementId>,
    clipboard: Box<dyn Clipboard>,
    clipboard_target: Rc<()>,
    queue: Queue,
    keymap: Option<Keymap>,
    surfaces: Vec<ElementId>,
    surface_ops: Vec<SurfaceOp>,
    surface_sinks: FxHashMap<SurfaceId, StatusSink>,
    window_visible: bool,
    selection_scopes: Vec<ElementId>,
    selection_drag: Option<SelectionDrag>,
    access: Option<Box<Access>>,
}

impl Tree {
    pub(crate) fn new(size: Size, queue: Queue) -> Tree {
        let mut tree = Tree {
            elements: SlotMap::with_key(),
            fragments: SlotMap::with_key(),
            layouts: SecondaryMap::new(),
            root: ElementId::default(),
            content: FragmentId::default(),
            size,
            time: 0.0,
            dirty: Dirty::Layout,
            hover_path: Vec::new(),
            press_path: Vec::new(),
            capture: None,
            stats: FrameStats::default(),
            layers: Layers::new(),
            repaint_all: true,
            observers: Vec::new(),
            dirty_styles: Vec::new(),
            themes: Vec::new(),
            scrolled: Vec::new(),
            motions: FxHashMap::default(),
            focused: None,
            replaced_focus: None,
            focus_visible: false,
            restore: Vec::new(),
            focus_fallback: None,
            handles: FxHashMap::default(),
            overlay: None,
            clipboard: Box::new(MemoryClipboard::new()),
            clipboard_target: Rc::new(()),
            queue,
            keymap: None,
            surfaces: Vec::new(),
            surface_ops: Vec::new(),
            surface_sinks: FxHashMap::default(),
            window_visible: true,
            selection_scopes: Vec::new(),
            selection_drag: None,
            access: None,
        };
        tree.root = tree.create_element(ElementSpec {
            layout: root_layout(size),
            layer: Some(LayerSpec::default()),
            ..Default::default()
        });
        if let Some(layer) = tree.elements[tree.root].layer(Slot::Own) {
            tree.layers.set_root(layer.id);
        }
        tree.content = tree.fragment(Node::Element(tree.root));
        tree
    }

    pub(crate) fn content(&self) -> Node {
        Node::Fragment(self.content)
    }

    pub(crate) fn root_background(&self) -> Option<Color> {
        let mut id = self.root;
        loop {
            let element = &self.elements[id];
            if let Some(background) = element.effective_style().background {
                return Some(background.resolve(&self.palette_at(id)));
            }
            id = *element.children.first()?;
        }
    }

    pub(crate) fn set_theme(&mut self, theme: Theme) {
        self.themes.clear();
        self.watch_theme(theme);
    }

    pub(crate) fn theme(&self) -> Option<Theme> {
        self.themes.first().map(|(theme, _)| *theme)
    }

    pub(crate) fn watch_theme(&mut self, theme: Theme) {
        if self.themes.iter().all(|(seen, _)| *seen != theme) {
            self.themes.push((theme, theme.get_untracked()));
        }
    }

    pub(crate) fn palette_at(&self, id: ElementId) -> Palette {
        theme_at(&self.elements, id)
            .or_else(|| self.theme())
            .map_or_else(
                || UiPalette::light().map(Color::from),
                |theme| theme.get_untracked(),
            )
    }

    fn sync_themes(&mut self) {
        let mut changed = false;
        self.themes.retain_mut(|(theme, palette)| {
            if !theme.is_alive() {
                return false;
            }
            let current = theme.get_untracked();
            changed |= replace(palette, current) != current;
            true
        });
        if changed {
            let ids: Vec<_> = self.elements.keys().collect();
            for id in ids {
                self.sync_backing(id);
            }
            self.invalidate_paint();
        }
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
            self.layouts[self.root].style = root_layout(size);
            self.invalidate_layout(self.root);
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
        let own = spec.layer.take().map(|layer| self.new_layer(layer));
        let extras = spec.extras.as_deref();
        let observer = extras
            .and_then(|extras| extras.scroll.as_ref())
            .map(|scroll| scroll.observer);
        let content = observer.map(|observer| {
            let layer = self.new_layer(LayerSpec::default());
            if let Some(state) = observer {
                self.layers.set_scroll_source(layer.id, scroll_key(state));
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
                    || extras.selection.is_some()
            })
            || selection.is_some_and(|edit| edit.selection_signal.is_some());
        let handle = extras
            .and_then(|extras| extras.focus)
            .and_then(|focus| focus.handle);
        let attach = match &mut spec.kind {
            Kind::Surface(surface) => Some(surface.attach.take()),
            _ => None,
        };
        let id = self.elements.insert(Element {
            style: Style::default(),
            resolved: None,
            pending: None,
            kind: spec.kind,
            extras: spec.extras,
            layers: [own.map(Box::new), content.map(Box::new)],
            parent: None,
            fragment: None,
            children: Vec::new(),
            bounds: Rect::ZERO,
            painted: Rect::ZERO,
            border: Edges::ZERO,
            states: [false; STATES],
            cursor: None,
            flags: spec.flags,
        });
        self.layouts.insert(id, LayoutNode::new(spec.layout));
        if watched {
            self.observers.push(id);
        }
        if self.elements[id]
            .extras()
            .is_some_and(|extras| extras.selection.is_some())
        {
            self.selection_scopes.push(id);
        }
        if let Some(attach) = attach {
            if let Some((source, status)) = attach {
                self.surface_sinks.insert(source.id(), status);
                self.surface_ops.push(SurfaceOp::Attach(source));
            }
            self.surfaces.push(id);
        }
        if let Some((handle, focused)) = handle {
            self.handles.insert(handle, (id, focused));
        }
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
        let p = &mut self.elements[parent];
        let index = index.min(p.children.len());
        p.children.insert(index, child);
        self.invalidate_layout(parent);
        self.elements[child].parent = Some(parent);
        if self.is_connected(parent) {
            self.refresh_variant_subtree(child);
            self.refresh_text_subtree(child);
            self.sync_backing_subtree(child);
            self.mark_access_subtree(child);
        }
        self.mark_layer(parent, true);
        self.mark(Dirty::Layout);
    }

    fn detach(&mut self, child: ElementId) {
        let Some(parent) = self.elements.get_mut(child).and_then(|c| c.parent.take()) else {
            return;
        };
        self.mark_layer(parent, true);
        self.mark_access(parent);
        self.elements[parent].children.retain(|c| *c != child);
        self.invalidate_layout(parent);
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
