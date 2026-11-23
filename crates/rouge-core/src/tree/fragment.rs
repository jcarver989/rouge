use super::layers::Slot;
use super::{ElementId, Exit, Kind, Presence, Tree};
use crate::compositor::SurfaceOp;
use crate::style::Dirty;
use slotmap::new_key_type;
use std::mem::take;

new_key_type! {
    pub(crate) struct FragmentId;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Node {
    Element(ElementId),
    Fragment(FragmentId),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Parent {
    Host(ElementId),
    Fragment(FragmentId),
    Detached,
}

pub(super) struct Fragment {
    parent: Parent,
    nodes: Vec<Node>,
}

impl Tree {
    pub(crate) fn fragment(&mut self, parent: Node) -> FragmentId {
        let parent = match parent {
            Node::Fragment(fragment) => fragment,
            Node::Element(host) => self.host_fragment(host),
        };
        let fragment = self.fragments.insert(Fragment {
            parent: Parent::Fragment(parent),
            nodes: Vec::new(),
        });
        if let Some(record) = self.fragments.get_mut(parent) {
            record.nodes.push(Node::Fragment(fragment));
        }
        fragment
    }

    pub(crate) fn detached_fragment(&mut self) -> FragmentId {
        self.fragments.insert(Fragment {
            parent: Parent::Detached,
            nodes: Vec::new(),
        })
    }

    pub(crate) fn append(&mut self, parent: Node, child: ElementId) {
        let fragment = match parent {
            Node::Fragment(fragment) => fragment,
            Node::Element(host) => match self.elements[host].fragment {
                Some(fragment) => fragment,
                None => return self.attach(host, usize::MAX, child),
            },
        };
        if !self.fragments.contains_key(fragment) {
            return self.discard(vec![child], None);
        }
        let host = self.host(fragment);
        let index = host.map(|_| self.end(fragment));
        self.fragments[fragment].nodes.push(Node::Element(child));
        if let (Some(host), Some(index)) = (host, index) {
            self.attach(host, index, child);
        }
    }

    pub(crate) fn present(&mut self, fragment: FragmentId, presence: Presence) {
        if presence.enter.is_none() && presence.exit.is_none() {
            return;
        }
        let mut tops = Vec::new();
        self.flatten(Node::Fragment(fragment), &mut tops);
        for id in tops {
            self.promote(id, presence);
        }
    }

    pub(crate) fn reorder(&mut self, fragment: FragmentId, nodes: Vec<Node>) {
        if !self.fragments.contains_key(fragment) {
            return;
        }
        for node in &nodes {
            if let Node::Fragment(child) = node
                && let Some(record) = self.fragments.get_mut(*child)
            {
                record.parent = Parent::Fragment(fragment);
            }
        }
        self.fragments[fragment].nodes = nodes;
        let Some(host) = self.host(fragment) else {
            return;
        };
        let start = self.start(fragment);
        let mut desired = Vec::new();
        self.flatten(Node::Fragment(fragment), &mut desired);
        for (offset, id) in desired.into_iter().enumerate() {
            if self.elements[host].children.get(start + offset) != Some(&id) {
                self.detach(id);
                self.attach(host, start + offset, id);
            }
        }
    }

    pub(crate) fn clear(&mut self, fragment: FragmentId, exit: Exit) {
        let Some(record) = self.fragments.get_mut(fragment) else {
            return;
        };
        let nodes = take(&mut record.nodes);
        let mut tops = Vec::new();
        for node in &nodes {
            self.flatten(*node, &mut tops);
        }
        for node in nodes {
            if let Node::Fragment(child) = node {
                self.free(child);
            }
        }
        self.replace_focus(&tops);
        if let Some(host) = self.host(fragment) {
            let start = self.start(fragment);
            self.detach_range(host, start, tops.len());
        }
        self.discard(tops, exit);
    }

    pub(crate) fn remove_fragment(&mut self, fragment: FragmentId, exit: Exit) {
        self.clear(fragment, exit);
        let Some(record) = self.fragments.remove(fragment) else {
            return;
        };
        match record.parent {
            Parent::Fragment(parent) => {
                if let Some(parent) = self.fragments.get_mut(parent) {
                    parent
                        .nodes
                        .retain(|node| *node != Node::Fragment(fragment));
                }
            }
            Parent::Host(host) => {
                if let Some(host) = self.elements.get_mut(host) {
                    host.fragment = None;
                }
            }
            Parent::Detached => {}
        }
    }

    fn free(&mut self, fragment: FragmentId) {
        if let Some(record) = self.fragments.remove(fragment) {
            for node in record.nodes {
                if let Node::Fragment(child) = node {
                    self.free(child);
                }
            }
        }
    }

    fn host_fragment(&mut self, host: ElementId) -> FragmentId {
        if let Some(fragment) = self.elements[host].fragment {
            return fragment;
        }
        let nodes = self.elements[host]
            .children
            .iter()
            .copied()
            .map(Node::Element)
            .collect();
        let fragment = self.fragments.insert(Fragment {
            parent: Parent::Host(host),
            nodes,
        });
        self.elements[host].fragment = Some(fragment);
        fragment
    }

    fn host(&self, mut fragment: FragmentId) -> Option<ElementId> {
        loop {
            match self.fragments.get(fragment)?.parent {
                Parent::Host(host) => return Some(host),
                Parent::Fragment(parent) => fragment = parent,
                Parent::Detached => return None,
            }
        }
    }

    fn start(&self, fragment: FragmentId) -> usize {
        let Parent::Fragment(parent) = self.fragments[fragment].parent else {
            return 0;
        };
        let siblings = self.fragments[parent].nodes.iter();
        let before = siblings.take_while(|node| **node != Node::Fragment(fragment));
        self.start(parent) + before.map(|node| self.len(*node)).sum::<usize>()
    }

    fn end(&self, fragment: FragmentId) -> usize {
        let mut tail = fragment;
        loop {
            match self.fragments[tail].parent {
                Parent::Host(host) => return self.elements[host].children.len(),
                Parent::Fragment(parent)
                    if self.fragments[parent].nodes.last() == Some(&Node::Fragment(tail)) =>
                {
                    tail = parent;
                }
                _ => return self.start(fragment) + self.len(Node::Fragment(fragment)),
            }
        }
    }

    fn len(&self, node: Node) -> usize {
        match node {
            Node::Element(_) => 1,
            Node::Fragment(fragment) => self.fragments.get(fragment).map_or(0, |record| {
                record.nodes.iter().map(|node| self.len(*node)).sum()
            }),
        }
    }

    fn flatten(&self, node: Node, out: &mut Vec<ElementId>) {
        match node {
            Node::Element(id) => out.push(id),
            Node::Fragment(fragment) => {
                for node in self
                    .fragments
                    .get(fragment)
                    .into_iter()
                    .flat_map(|f| &f.nodes)
                {
                    self.flatten(*node, out);
                }
            }
        }
    }

    fn detach_range(&mut self, parent: ElementId, start: usize, count: usize) {
        if count == 0 {
            return;
        }
        self.mark_layer(parent, true);
        self.mark_access(parent);
        let range = start..start + count;
        let removed: Vec<_> = self.elements[parent].children.drain(range).collect();
        self.invalidate_layout(parent);
        for id in removed {
            self.elements[id].parent = None;
        }
        self.mark(Dirty::Layout);
    }

    fn discard(&mut self, tops: Vec<ElementId>, exit: Exit) {
        if tops.is_empty() {
            return;
        }
        for top in &tops {
            self.detach(*top);
        }
        for top in tops {
            let layer = self.elements.get(top).and_then(|e| e.layer(Slot::Own));
            let exits = match (exit, layer) {
                (Some((to, timing)), Some(layer)) => {
                    self.layers.exit(layer.id, to, timing);
                    true
                }
                _ => false,
            };
            let mut stack = vec![top];
            while let Some(element) = stack.pop().and_then(|id| {
                if let Some(access) = &mut self.access {
                    access.editors.remove(&id);
                }
                self.layouts.remove(id);
                self.elements.remove(id)
            }) {
                self.stats.elements_removed += 1;
                if let Kind::Surface(surface) = &element.kind {
                    self.surface_sinks.remove(&surface.id);
                    self.surface_ops.push(SurfaceOp::Detach(surface.id));
                }
                for layer in element.layers.into_iter().flatten().filter(|_| !exits) {
                    self.layers.remove_layer(layer.id);
                }
                if let Some(fragment) = element.fragment {
                    self.free(fragment);
                }
                stack.extend(element.children);
            }
        }
        let elements = &self.elements;
        for list in [
            &mut self.hover_path,
            &mut self.press_path,
            &mut self.observers,
            &mut self.surfaces,
            &mut self.selection_scopes,
        ] {
            list.retain(|e| elements.contains_key(*e));
        }
        if self
            .selection_drag
            .is_some_and(|drag| !elements.contains_key(drag.scope))
        {
            self.selection_drag = None;
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|path| path.iter().any(|e| !elements.contains_key(*e)))
        {
            self.capture = None;
        }
        self.forget_focus();
    }
}
