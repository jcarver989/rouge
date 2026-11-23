use super::events::On;
use super::layers::Slot;
use super::{Element, ElementId, Interaction, Kind, TextData, Tree};
use crate::access::{AccessOp, Decrement, Increment, Semantics};
use crate::text::{TextSelection, access::TextRuns};
use accesskit::{
    Action, Affine as NodeTransform, Node, NodeId, Rect as NodeRect, Role,
    TextSelection as AccessSelection, TreeId, TreeInfo, TreeUpdate,
};
use rouge_render::{Affine, Point, Rect};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::{Key, KeyData};
use std::any::TypeId;

const CONTENT: u64 = 1 << 63;
const TOOLKIT: &str = "Rouge";

type Poses = [Option<Affine>; 2];

#[derive(Default)]
pub(super) struct Access {
    pub(super) dirty: Vec<ElementId>,
    poses: FxHashMap<ElementId, Poses>,
    pub(super) editors: FxHashMap<ElementId, TextRuns>,
    focus: Option<NodeId>,
    scale: f32,
}

impl Access {
    pub(super) fn text_moved(&self, id: ElementId, text: &TextData, origin: Point) -> bool {
        let stale = |layout| {
            let runs = self.editors.get(&id);
            runs.is_none_or(|runs| !runs.matches(layout, origin + text.offset()))
        };
        text.edit.is_some() && text.layout.as_ref().is_some_and(stale)
    }
}

fn node_id(id: ElementId) -> NodeId {
    NodeId(id.data().as_ffi())
}

fn content_id(id: ElementId) -> NodeId {
    NodeId(id.data().as_ffi() | CONTENT)
}

fn node_rect(rect: Rect) -> NodeRect {
    let max = rect.max();
    NodeRect {
        x0: f64::from(rect.origin.x),
        y0: f64::from(rect.origin.y),
        x1: f64::from(max.x),
        y1: f64::from(max.y),
    }
}

fn node_transform(affine: Affine) -> NodeTransform {
    let Affine { a, b, c, d, tx, ty } = affine;
    NodeTransform::new([a, b, c, d, tx, ty].map(f64::from))
}

fn checkable(role: Role) -> bool {
    matches!(
        role,
        Role::CheckBox
            | Role::Switch
            | Role::RadioButton
            | Role::MenuItemCheckBox
            | Role::MenuItemRadio
    )
}

fn named_by_content(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::DefaultButton
            | Role::Link
            | Role::CheckBox
            | Role::RadioButton
            | Role::Switch
            | Role::Tab
            | Role::MenuItem
            | Role::MenuItemCheckBox
            | Role::MenuItemRadio
            | Role::ListBoxOption
            | Role::TreeItem
            | Role::ListItem
            | Role::Row
            | Role::Cell
            | Role::ColumnHeader
            | Role::RowHeader
            | Role::Heading
    )
}

fn selectable(role: Role) -> bool {
    matches!(
        role,
        Role::Tab | Role::ListBoxOption | Role::Row | Role::Cell | Role::TreeItem
    )
}

impl Element {
    fn semantics(&self) -> Option<&Semantics> {
        self.extras()?.access.as_deref()
    }

    fn handles(&self, on: On) -> bool {
        self.extras()
            .is_some_and(|extras| extras.listeners.iter().any(|(kind, _)| *kind == on))
    }

    fn acts_on<T: 'static>(&self) -> bool {
        let kind = TypeId::of::<T>();
        self.extras()
            .is_some_and(|extras| extras.actions.iter().any(|(id, _)| *id == kind))
    }

    fn editable(&self) -> bool {
        self.kind.text().is_some_and(|text| text.edit.is_some())
    }

    fn inaccessible(&self) -> bool {
        self.unseen() || self.semantics().is_some_and(|semantics| semantics.hidden)
    }

    fn interactive(&self) -> bool {
        self.handles(On::Click) || self.extras().is_some_and(|extras| extras.focus.is_some())
    }

    fn names_itself(&self, root: bool) -> bool {
        self.kind.text().is_none()
            && self
                .semantics()
                .is_some_and(|semantics| semantics.node.label().is_none())
            && named_by_content(self.role(root))
    }

    fn is_node(&self) -> bool {
        matches!(self.kind, Kind::Text(_))
            || self.layers.iter().any(Option::is_some)
            || self.interactive()
            || self
                .extras()
                .is_some_and(|e| e.access.is_some() || e.scroll.is_some())
    }

    fn role(&self, root: bool) -> Role {
        let semantics = self.semantics();
        if let Some(role) = semantics.and_then(|s| s.role) {
            return role;
        }
        let extras = self.extras();
        match &self.kind {
            _ if root => Role::Window,
            Kind::Text(text) => match &text.edit {
                Some(edit) if edit.multiline => Role::MultilineTextInput,
                Some(_) => Role::TextInput,
                None => Role::Label,
            },
            Kind::Image(_) if semantics.is_some() => Role::Image,
            _ if extras.is_some_and(|extras| extras.scroll.is_some()) => Role::ScrollView,
            _ if semantics.is_some_and(|s| s.activates) => Role::Button,
            _ if self.interactive() => Role::Unknown,
            _ => Role::GenericContainer,
        }
    }
}

impl Tree {
    pub(super) fn mark_access(&mut self, id: ElementId) {
        if let Some(access) = &mut self.access {
            access.dirty.push(id);
        }
    }

    pub(super) fn mark_access_subtree(&mut self, id: ElementId) {
        let Some(access) = &mut self.access else {
            return;
        };
        let mut pending = vec![id];
        while let Some(element) = pending.pop().and_then(|id| {
            access.dirty.push(id);
            self.elements.get(id)
        }) {
            pending.extend(&element.children);
        }
        access
            .dirty
            .extend(self.elements.get(id).and_then(|element| element.parent));
    }

    pub(crate) fn set_access(&mut self, id: ElementId, op: AccessOp) {
        let Some(element) = self.elements.get_mut(id) else {
            return;
        };
        let structural = matches!(op, AccessOp::Hidden(_));
        let extras = element.extras.get_or_insert_default();
        extras.access.get_or_insert_default().apply(op);
        if structural {
            self.mark_access_subtree(id);
        } else {
            self.mark_access(id);
        }
    }

    pub(crate) fn stop_access(&mut self) {
        self.access = None;
    }

    pub(crate) fn access_tree(&mut self, scale: f32) -> TreeUpdate {
        let access = Box::new(Access {
            scale,
            ..Access::default()
        });
        self.project_access(access, vec![self.root], true)
    }

    pub(crate) fn access_update(&mut self, scale: f32) -> Option<TreeUpdate> {
        let mut access = self.access.take()?;
        let mut owners = Vec::new();
        let mut seen = FxHashSet::default();
        if access.scale != scale {
            access.scale = scale;
            access.dirty.push(self.root);
        }
        for id in access.dirty.drain(..) {
            let Some(path) = self.visible_access_path(id) else {
                access.editors.remove(&id);
                continue;
            };
            let ancestors = path.iter().rev().copied();
            let owner = ancestors.clone().find(|&id| self.is_access_node(id));
            let namers = ancestors
                .skip(1)
                .filter(|&id| self.elements[id].names_itself(id == self.root));
            for id in owner.into_iter().chain(namers) {
                if seen.insert(id) {
                    owners.push(id);
                }
            }
        }
        access.poses.retain(|&id, poses| {
            if self.visible_access_path(id).is_none() || !self.is_access_node(id) {
                return false;
            }
            if self.access_poses(id) != *poses && seen.insert(id) {
                owners.push(id);
            }
            true
        });
        owners.reverse();
        let previous = access.focus;
        let update = self.project_access(access, owners, false);
        (!update.nodes.is_empty() || previous != Some(update.focus)).then_some(update)
    }

    fn project_access(
        &mut self,
        mut access: Box<Access>,
        mut pending: Vec<ElementId>,
        initial: bool,
    ) -> TreeUpdate {
        let mut nodes = Vec::new();
        while let Some(id) = pending.pop() {
            let children = self.access_children(id);
            if initial {
                pending.extend(children.iter().rev());
            }
            self.push_access_nodes(id, children, &mut access, &mut nodes);
        }
        let focus = self.access_focus();
        access.focus = Some(focus);
        self.access = Some(access);
        self.stats.access_nodes += nodes.len() as u64;
        TreeUpdate {
            nodes,
            tree: initial.then(|| TreeInfo {
                root: node_id(self.root),
                toolkit_name: Some(TOOLKIT.to_owned()),
                toolkit_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
            tree_id: TreeId::ROOT,
            focus,
        }
    }

    fn visible_access_path(&self, id: ElementId) -> Option<Vec<ElementId>> {
        let path = self.chain(id);
        (path.first() == Some(&self.root)
            && !path.iter().any(|&id| self.elements[id].inaccessible()))
        .then_some(path)
    }

    fn is_access_node(&self, id: ElementId) -> bool {
        id == self.root || self.elements[id].is_node()
    }

    pub(crate) fn access_target(&self, node: NodeId) -> Option<(ElementId, Vec<ElementId>)> {
        let id = ElementId::from(KeyData::from_ffi(node.0 & !CONTENT));
        let path = self.visible_access_path(id)?;
        (self.is_access_node(id) && (node == node_id(id) || node == content_id(id)))
            .then_some((id, path))
    }

    pub(crate) fn resolve_selection(
        &self,
        id: ElementId,
        target: NodeId,
        selection: AccessSelection,
    ) -> Option<TextSelection> {
        if target != node_id(id) || !self.accepts_value(id) {
            return None;
        }
        let layout = self.elements[id].kind.text()?.layout.as_ref()?;
        self.access
            .as_ref()?
            .editors
            .get(&id)?
            .resolve(layout, selection)
    }

    pub(crate) fn accepts_value(&self, id: ElementId) -> bool {
        self.elements.get(id).is_some_and(Element::editable) && !self.is_disabled(id)
    }

    pub(crate) fn reveal(&mut self, id: ElementId) {
        for scroller in self.chain(id).into_iter().rev().skip(1) {
            let Some(state) = self
                .layer_id(scroller, Slot::Content)
                .and_then(|layer| self.layers.scroll(layer))
            else {
                continue;
            };
            let (target, port) = (self.window_bounds(id), self.window_bounds(scroller));
            let along = |start: f32, end: f32, low: f32, high: f32| {
                if start < low || end - start > high - low {
                    start - low
                } else {
                    (end - high).max(0.0)
                }
            };
            let (from, to) = (target.origin, target.max());
            let (low, high) = (port.origin, port.max());
            let delta = Point::new(
                along(from.x, to.x, low.x, high.x),
                along(from.y, to.y, low.y, high.y),
            );
            if delta != Point::ZERO {
                self.scroll_to(scroller, state.offset + delta, false);
            }
        }
    }

    fn access_focus(&self) -> NodeId {
        let focused = self
            .focused
            .filter(|&id| self.visible_access_path(id).is_some() && self.is_access_node(id));
        node_id(focused.unwrap_or(self.root))
    }

    fn access_text(&self, id: ElementId) -> String {
        let mut words: Vec<&str> = Vec::new();
        let mut pending: Vec<_> = self.elements[id].children.iter().rev().copied().collect();
        while let Some(id) = pending.pop() {
            let element = &self.elements[id];
            if element.inaccessible() || element.interactive() {
                continue;
            }
            match element.kind.text() {
                Some(text) if !text.content.trim().is_empty() => words.push(text.content.trim()),
                _ => pending.extend(element.children.iter().rev()),
            }
        }
        words.join(" ")
    }

    fn access_children(&self, id: ElementId) -> Vec<ElementId> {
        let mut children = Vec::new();
        let mut pending: Vec<_> = self.elements[id].children.iter().rev().copied().collect();
        while let Some(id) = pending.pop() {
            let element = &self.elements[id];
            if element.inaccessible() {
                continue;
            }
            if element.is_node() {
                children.push(id);
            } else {
                pending.extend(element.children.iter().rev());
            }
        }
        children
    }

    fn access_poses(&self, id: ElementId) -> Poses {
        [Slot::Own, Slot::Content].map(|slot| self.enter(id, slot))
    }

    fn push_access_nodes(
        &self,
        id: ElementId,
        children: Vec<ElementId>,
        access: &mut Access,
        nodes: &mut Vec<(NodeId, Node)>,
    ) {
        let poses = self.access_poses(id);
        let mut node = self.access_node(id);
        let mut children: Vec<_> = children.into_iter().map(node_id).collect();
        if let [own, content] = poses
            && (own.is_some() || content.is_some())
        {
            access.poses.insert(id, poses);
            let window = if id == self.root { access.scale } else { 1.0 };
            let own = own.unwrap_or_default().then(&Affine::scale(window, window));
            if own != Affine::IDENTITY {
                node.set_transform(node_transform(own));
            }
            if let Some(content) = content {
                let mut inner = Node::new(Role::GenericContainer);
                inner.set_transform(node_transform(content));
                inner.set_children(children);
                nodes.push((content_id(id), inner));
                children = vec![content_id(id)];
            }
        }
        let element = &self.elements[id];
        if let Some(text) = element.kind.text()
            && let (Some(edit), Some(layout)) = (&text.edit, &text.layout)
        {
            let origin = element.bounds.origin + text.offset();
            let runs = access.editors.entry(id).or_default();
            runs.publish(layout, &text.content, origin, nodes);
            children.extend(runs.children());
            if let Some(selection) = runs.selection(edit.current(layout, &text.content)) {
                node.set_text_selection(selection);
                if !self.is_disabled(id) {
                    node.add_action(Action::SetTextSelection);
                }
            }
        }
        node.set_children(children);
        nodes.push((node_id(id), node));
    }

    fn access_node(&self, id: ElementId) -> Node {
        let element = &self.elements[id];
        let role = element.role(id == self.root);
        let mut node = element.semantics().map_or_default(|s| s.node.clone());
        node.set_role(role);
        node.set_bounds(node_rect(element.bounds));
        if element.clips_children() {
            node.set_clips_children();
        }
        if self.is_disabled(id) {
            node.set_disabled();
        }
        let checked = element.states[Interaction::Checked as usize];
        let selected = element.states[Interaction::Selected as usize];
        if node.toggled().is_none() && checkable(role) {
            node.set_toggled(checked.into());
        }
        if selected || selectable(role) {
            node.set_selected(selected);
        }
        if let Some(text) = element.kind.text() {
            if role == Role::Label || text.edit.is_some() {
                if node.value().is_none() {
                    node.set_value(text.content.as_str());
                }
            } else if node.label().is_none() {
                node.set_label(text.content.as_str());
            }
            if text.edit.is_some() && !text.placeholder.is_empty() {
                node.set_placeholder(text.placeholder.as_str());
            }
        }
        if element.flags.focus_scope && matches!(role, Role::Dialog | Role::AlertDialog) {
            node.set_modal();
        }
        if element.names_itself(id == self.root) {
            let name = self.access_text(id);
            if !name.is_empty() {
                node.set_label(name);
            }
        }
        if let Some(state) = self
            .layer_id(id, Slot::Content)
            .and_then(|layer| self.layers.scroll(layer))
        {
            let reach = |content: f32, viewport: f32| f64::from((content - viewport).max(0.0));
            node.set_scroll_x(f64::from(state.offset.x));
            node.set_scroll_y(f64::from(state.offset.y));
            node.set_scroll_x_min(0.0);
            node.set_scroll_y_min(0.0);
            node.set_scroll_x_max(reach(state.content.width, state.viewport.width));
            node.set_scroll_y_max(reach(state.content.height, state.viewport.height));
        }
        let focusable = element
            .extras()
            .is_some_and(|extras| extras.focus.is_some());
        let actions = [
            (Action::Click, element.handles(On::Click)),
            (Action::Focus, focusable),
            (Action::Increment, element.acts_on::<Increment>()),
            (Action::Decrement, element.acts_on::<Decrement>()),
            (Action::SetValue, element.editable()),
            (Action::ReplaceSelectedText, element.editable()),
            (Action::ScrollIntoView, id != self.root),
        ];
        for (action, supported) in actions {
            if supported {
                node.add_action(action);
            }
        }
        node
    }
}
