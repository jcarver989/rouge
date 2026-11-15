use super::content::TextData;
use super::layers::{LayerRef, LayerState, Slot};
use super::paint::layer_mut;
use super::{Element, ElementId, Kind, Tree};
use crate::canvas::CanvasMeasure;
use crate::image::ImageSpec;
use crate::style::{Dirty, base_layout};
use crate::text::TextSystem;
use rouge_render::{Edges, Point, Rect, Size};
use slotmap::{Key, KeyData, SecondaryMap, SlotMap};
use std::iter::{Copied, Map};
use std::mem::replace;
use std::slice::Iter;
use taffy::{
    AvailableSpace, BlockContext, Cache, CacheTree, ClearState, Dimension, Display,
    Layout as Placement, LayoutBlockContainer, LayoutContainingBlock, LayoutFlexboxContainer,
    LayoutGridContainer, LayoutInput, LayoutOutput, LayoutPartialTree, Line, NodeId,
    Point as Offset, Rect as Sides, RunMode, Size as Extent, Style as Layout, TraversePartialTree,
    compute_block_layout, compute_cached_layout, compute_flexbox_layout, compute_grid_layout,
    compute_hidden_layout, compute_leaf_layout, compute_oof_layout, compute_root_layout,
};
use tracing::trace_span;

const WRAP_TOLERANCE: f32 = 1.0;

#[derive(Clone, Copy)]
struct Space {
    origin: Point,
    layer: LayerRef,
}

struct LayoutCx<'a> {
    text_system: &'a mut TextSystem,
    moved: Vec<LayerRef>,
}

#[derive(Clone, Copy)]
struct Parent {
    origin: Point,
    unrounded: Offset<f32>,
    containing: Offset<f32>,
}

#[repr(C)]
pub(super) struct LayoutNode {
    unrounded: Placement,
    pub(super) style: Layout,
    cache: Cache,
}

impl LayoutNode {
    pub(super) fn new(style: Layout) -> LayoutNode {
        LayoutNode {
            unrounded: Placement::new(),
            style,
            cache: Cache::new(),
        }
    }
}

struct LayoutTree<'a> {
    elements: &'a mut SlotMap<ElementId, Element>,
    layouts: &'a mut SecondaryMap<ElementId, LayoutNode>,
    text_system: &'a mut TextSystem,
}

#[inline]
fn node(id: ElementId) -> NodeId {
    NodeId::from(id.data().as_ffi())
}

#[inline]
fn element(node: NodeId) -> ElementId {
    KeyData::from_ffi(node.into()).into()
}

impl Tree {
    pub(crate) fn layout(&mut self, text_system: &mut TextSystem) -> bool {
        self.resolve_styles();
        if !self.dirty.needs_layout() {
            return false;
        }
        let _layout = trace_span!("layout").entered();
        let root = node(self.root);
        let available = Extent {
            width: AvailableSpace::Definite(self.size.width),
            height: AvailableSpace::Definite(self.size.height),
        };
        let mut tree = LayoutTree {
            elements: &mut self.elements,
            layouts: &mut self.layouts,
            text_system: &mut *text_system,
        };
        compute_root_layout(&mut tree, root, available);
        let mut cx = LayoutCx {
            text_system,
            moved: Vec::new(),
        };
        let space = Space {
            origin: Point::ZERO,
            layer: (self.root, Slot::Own),
        };
        let parent = Parent {
            origin: Point::ZERO,
            unrounded: Offset::ZERO,
            containing: Offset::ZERO,
        };
        self.resolve_bounds(self.root, parent, space, &mut cx);
        for layer in cx.moved {
            if let Some(layer) = layer_mut(&mut self.elements, layer) {
                layer.dirty = true;
            }
        }
        self.stats.layouts += 1;
        self.dirty = Dirty::Paint;
        true
    }

    fn resolve_bounds(
        &mut self,
        id: ElementId,
        parent: Parent,
        mut space: Space,
        cx: &mut LayoutCx<'_>,
    ) {
        let node = &self.layouts[id];
        let style = &node.style;
        let out_of_flow = style.position.is_out_of_flow() && style.display != Display::None;
        let base = if out_of_flow {
            parent.containing
        } else {
            parent.unrounded
        };
        let (layout, unrounded) = snap(&node.unrounded, base);
        let containing = if id == self.root || style.position.is_positioned() {
            unrounded
        } else {
            parent.containing
        };
        let element = &mut self.elements[id];
        let (padding, border) = (layout.padding, layout.border);
        if let Some(text) = element.kind.text_mut() {
            let content_width =
                layout.size.width - padding.left - padding.right - border.left - border.right;
            let content_height =
                layout.size.height - padding.top - padding.bottom - border.top - border.bottom;
            text.inset = Point::new(padding.left + border.left, padding.top + border.top);
            text.wrap_width = Some(content_width + WRAP_TOLERANCE);
            text.content_size = Size::new(content_width.max(0.0), content_height.max(0.0));
            let wrap_width = text.wrap_width;
            text.laid_out(cx.text_system, wrap_width);
            if let (Some(edit), Some(layout)) = (&mut text.edit, &text.layout) {
                edit.selection = edit.current(layout, &text.content);
                edit.stale = false;
                edit.scroll_to_caret(layout, &text.content, text.content_size);
            }
        }
        let origin = parent.origin + Point::new(layout.location.x, layout.location.y);
        let size = Size::new(layout.size.width, layout.size.height);
        let border = Edges {
            top: border.top,
            right: border.right,
            bottom: border.bottom,
            left: border.left,
        };
        element.bounds = Rect { origin, size };
        let within = |space: &Space| Rect {
            origin: origin - space.origin,
            size,
        };
        let mut enter = |layer: &mut Option<Box<LayerState>>, slot: Slot, space: &mut Space| {
            let layer = layer.as_mut()?;
            let frame = within(space);
            if layer.frame.replace(frame) != Some(frame) {
                self.compositor.set_frame(layer.id, frame);
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
        if let Some(scroller) = enter(content, Slot::Content, &mut space)
            && let Some(scroll) = element.scroll_mut()
        {
            let viewport = Size::new(
                (size.width - border.left - border.right).max(0.0),
                (size.height - border.top - border.bottom).max(0.0),
            );
            let content = Size::new(
                viewport.width + layout.scroll_width(),
                viewport.height + layout.scroll_height(),
            );
            if replace(&mut scroll.extent, (viewport, content)) != (viewport, content) {
                self.compositor
                    .set_scroll_extent(scroller, viewport, content);
                if let Some(target) = scroll.target {
                    self.compositor.scroll_to(scroller, target);
                }
            }
        }
        let parent = Parent {
            origin,
            unrounded,
            containing,
        };
        for i in 0..element.children.len() {
            let child = self.elements[id].children[i];
            self.resolve_bounds(child, parent, space, cx);
        }
    }

    pub(super) fn invalidate_layout(&mut self, id: ElementId) {
        let mut current = Some(id);
        while let Some(id) = current {
            let Some(ClearState::Cleared) = self.layouts.get_mut(id).map(|node| node.cache.clear())
            else {
                return;
            };
            current = self.elements[id].parent;
        }
    }
}

impl LayoutTree<'_> {
    #[inline]
    fn node(&self, id: NodeId) -> &LayoutNode {
        &self.layouts[element(id)]
    }

    #[inline]
    fn node_mut(&mut self, id: NodeId) -> &mut LayoutNode {
        &mut self.layouts[element(id)]
    }

    fn lay_out(
        &mut self,
        id: NodeId,
        inputs: LayoutInput,
        block: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        if inputs.run_mode == RunMode::PerformHiddenLayout {
            return compute_hidden_layout(self, id);
        }
        compute_cached_layout(self, id, inputs, |tree, id, inputs| {
            let display = tree.node(id).style.display;
            let parent = tree.child_count(id) > 0;
            let mut output = match (display, parent) {
                (Display::None, _) => compute_hidden_layout(tree, id),
                (Display::Block, true) => compute_block_layout(tree, id, inputs, block),
                (Display::FlowRoot, true) => compute_block_layout(tree, id, inputs, None),
                (Display::Flex, true) => compute_flexbox_layout(tree, id, inputs),
                (Display::Grid, true) => compute_grid_layout(tree, id, inputs),
                (_, false) => tree.measure(id, inputs),
            };
            if inputs.run_mode == RunMode::PerformLayout {
                compute_oof_layout(tree, id, &mut output);
            }
            output
        })
    }

    fn measure(&mut self, id: NodeId, inputs: LayoutInput) -> LayoutOutput {
        let id = element(id);
        let kind = &mut self.elements[id].kind;
        let text_system = &mut *self.text_system;
        compute_leaf_layout(
            inputs,
            &self.layouts[id].style,
            |_, _| 0.0,
            |known, available| match kind {
                Kind::Text(text) => measure_text(text, text_system, known, available),
                Kind::Image(image) => measure_image(image, known),
                Kind::Canvas(canvas) => match &mut canvas.measure {
                    Some(measure) => measure_canvas(measure, text_system, known, available),
                    None => Extent::ZERO,
                },
                Kind::Div | Kind::Surface(_) => Extent::ZERO,
            },
        )
    }
}

impl TraversePartialTree for LayoutTree<'_> {
    type ChildIter<'a>
        = Map<Copied<Iter<'a, ElementId>>, fn(ElementId) -> NodeId>
    where
        Self: 'a;

    #[inline]
    fn child_ids(&self, parent: NodeId) -> Self::ChildIter<'_> {
        self.elements[element(parent)]
            .children
            .iter()
            .copied()
            .map(node as fn(ElementId) -> NodeId)
    }

    #[inline]
    fn child_count(&self, parent: NodeId) -> usize {
        self.elements[element(parent)].children.len()
    }

    #[inline]
    fn get_child_id(&self, parent: NodeId, index: usize) -> NodeId {
        node(self.elements[element(parent)].children[index])
    }
}

impl CacheTree for LayoutTree<'_> {
    #[inline]
    fn cache_get(&mut self, id: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.node_mut(id).cache.get(input)
    }

    #[inline]
    fn cache_store(&mut self, id: NodeId, input: &LayoutInput, output: LayoutOutput) {
        self.node_mut(id).cache.store(input, output);
    }

    #[inline]
    fn cache_clear(&mut self, id: NodeId) {
        self.node_mut(id).cache.clear();
    }
}

impl LayoutPartialTree for LayoutTree<'_> {
    type CoreContainerStyle<'a>
        = &'a Layout
    where
        Self: 'a;

    type CustomIdent = String;

    #[inline]
    fn get_core_container_style(&self, id: NodeId) -> &Layout {
        &self.node(id).style
    }

    #[inline]
    fn set_unrounded_layout(&mut self, id: NodeId, layout: &Placement) {
        self.node_mut(id).unrounded = *layout;
    }

    #[inline]
    fn compute_child_layout(&mut self, id: NodeId, inputs: LayoutInput) -> LayoutOutput {
        self.lay_out(id, inputs, None)
    }
}

impl LayoutContainingBlock for LayoutTree<'_> {
    type OofItemStyle<'a>
        = &'a Layout
    where
        Self: 'a;

    #[inline]
    fn get_oof_item_style(&self, id: NodeId) -> &Layout {
        &self.node(id).style
    }

    #[inline]
    fn clear_hoisted_children(&mut self, _: NodeId) {}

    #[inline]
    fn add_hoisted_children(&mut self, _: NodeId, _: &[NodeId]) {}
}

impl LayoutBlockContainer for LayoutTree<'_> {
    type BlockContainerStyle<'a>
        = &'a Layout
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a Layout
    where
        Self: 'a;

    #[inline]
    fn get_block_container_style(&self, id: NodeId) -> &Layout {
        &self.node(id).style
    }

    #[inline]
    fn get_block_child_style(&self, id: NodeId) -> &Layout {
        &self.node(id).style
    }

    #[inline]
    fn compute_block_child_layout(
        &mut self,
        id: NodeId,
        inputs: LayoutInput,
        block: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        self.lay_out(id, inputs, block)
    }
}

impl LayoutFlexboxContainer for LayoutTree<'_> {
    type FlexboxContainerStyle<'a>
        = &'a Layout
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = &'a Layout
    where
        Self: 'a;

    #[inline]
    fn get_flexbox_container_style(&self, id: NodeId) -> &Layout {
        &self.node(id).style
    }

    #[inline]
    fn get_flexbox_child_style(&self, id: NodeId) -> &Layout {
        &self.node(id).style
    }
}

impl LayoutGridContainer for LayoutTree<'_> {
    type GridContainerStyle<'a>
        = &'a Layout
    where
        Self: 'a;
    type GridItemStyle<'a>
        = &'a Layout
    where
        Self: 'a;

    #[inline]
    fn get_grid_container_style(&self, id: NodeId) -> &Layout {
        &self.node(id).style
    }

    #[inline]
    fn get_grid_child_style(&self, id: NodeId) -> &Layout {
        &self.node(id).style
    }
}

fn snap(unrounded: &Placement, parent: Offset<f32>) -> (Placement, Offset<f32>) {
    let Offset { x, y } = parent + unrounded.location;
    let size = unrounded.size;
    let span = |start: f32, length: f32, sides: Line<f32>| Line {
        start: (start + sides.start).round() - start.round(),
        end: (start + length).round() - (start + length - sides.end).round(),
    };
    let sides = |rect: Sides<f32>| {
        let horizontal = span(x, size.width, rect.horizontal_components());
        let vertical = span(y, size.height, rect.vertical_components());
        Sides {
            left: horizontal.start,
            right: horizontal.end,
            top: vertical.start,
            bottom: vertical.end,
        }
    };
    let overflow = unrounded.scrollable_overflow_rect;
    let layout = Placement {
        location: Offset {
            x: x.round() - parent.x.round(),
            y: y.round() - parent.y.round(),
        },
        size: Extent {
            width: (x + size.width).round() - x.round(),
            height: (y + size.height).round() - y.round(),
        },
        scrollbar_size: Extent {
            width: unrounded.scrollbar_size.width.round(),
            height: unrounded.scrollbar_size.height.round(),
        },
        border: sides(unrounded.border),
        padding: sides(unrounded.padding),
        scrollable_overflow_rect: Sides {
            left: (x + overflow.left).round() - x.round(),
            right: (x + overflow.right).round() - x.round(),
            top: (y + overflow.top).round() - y.round(),
            bottom: (y + overflow.bottom).round() - y.round(),
        },
        ..*unrounded
    };
    (layout, Offset { x, y })
}

pub(super) fn root_layout(size: Size) -> Layout {
    Layout {
        size: Extent {
            width: Dimension::length(size.width),
            height: Dimension::length(size.height),
        },
        ..base_layout()
    }
}

fn measure_text(
    text: &mut TextData,
    system: &mut TextSystem,
    known: Extent<Option<f32>>,
    available: Extent<AvailableSpace>,
) -> Extent<f32> {
    let constraint = known.width.or(match available.width {
        AvailableSpace::Definite(width) => Some(width),
        AvailableSpace::MinContent => Some(text.shaped(system).content_widths().0),
        AvailableSpace::MaxContent => None,
    });
    let (width, mut height) = text.laid_out(system, constraint).size();
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
