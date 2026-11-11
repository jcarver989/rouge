use crate::task::Executor;
use indexmap::IndexSet;
use slotmap::{SlotMap, new_key_type};
use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::mem::take;
use std::rc::Rc;

new_key_type! {
    pub(crate) struct NodeId;
}

const MAX_FLUSH_ROUNDS: usize = 1000;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
enum State {
    #[default]
    Clean,
    Check,
    Dirty,
}

pub(crate) type Value = Rc<RefCell<dyn Any>>;
pub(crate) type Compute = Rc<dyn Fn() -> bool>;

#[derive(Default)]
struct Node {
    state: State,
    value: Option<Value>,
    compute: Option<Compute>,
    sources: Vec<NodeId>,
    previous: Vec<NodeId>,
    computing: bool,
    subscribers: IndexSet<NodeId>,
    owner: Option<NodeId>,
    children: IndexSet<NodeId>,
    cleanups: Vec<Box<dyn FnOnce()>>,
    contexts: HashMap<TypeId, Rc<dyn Any>>,
}

#[derive(Default)]
pub(crate) struct Graph {
    nodes: RefCell<SlotMap<NodeId, Node>>,
    observer: Cell<Option<NodeId>>,
    owner: Cell<Option<NodeId>>,
    pending_effects: RefCell<Vec<NodeId>>,
    batch_depth: Cell<u32>,
    flushing: Cell<bool>,
    pub(crate) executor: Executor,
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Graph>>> = const { RefCell::new(None) };
}

pub(crate) fn try_with_rt<T>(f: impl FnOnce(&Graph) -> T) -> Option<T> {
    let graph = CURRENT.with(|current| current.borrow().clone())?;
    Some(f(&graph))
}

#[track_caller]
pub(crate) fn with_rt<T>(f: impl FnOnce(&Graph) -> T) -> T {
    match try_with_rt(f) {
        Some(result) => result,
        None => panic!(
            "rouge-reactive: no runtime is entered on this thread; create a `Runtime` and \
             hold the guard returned by `Runtime::enter` while using signals, effects and scopes"
        ),
    }
}

struct Owned(Rc<Graph>);

impl Drop for Owned {
    fn drop(&mut self) {
        let _entered = Entered::new(&self.0, None);
        self.0.teardown();
    }
}

pub struct Runtime {
    owned: Rc<Owned>,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime::owning(Graph::default())
    }
}

impl Runtime {
    pub fn new() -> Runtime {
        Runtime::default()
    }

    pub fn with_waker(waker: impl Fn() + Send + Sync + 'static) -> Runtime {
        Runtime::owning(Graph {
            executor: Executor::new(Some(Box::new(waker))),
            ..Graph::default()
        })
    }

    fn owning(graph: Graph) -> Runtime {
        Runtime {
            owned: Rc::new(Owned(Rc::new(graph))),
        }
    }

    pub fn enter(&self) -> Entered {
        Entered::new(&self.owned.0, Some(self.owned.clone()))
    }
}

#[must_use = "the runtime is only current while this guard is alive"]
pub struct Entered {
    previous: Option<Rc<Graph>>,
    _runtime: Option<Rc<Owned>>,
}

impl Entered {
    fn new(graph: &Rc<Graph>, runtime: Option<Rc<Owned>>) -> Entered {
        Entered {
            previous: CURRENT.with(|current| current.replace(Some(graph.clone()))),
            _runtime: runtime,
        }
    }
}

impl Drop for Entered {
    fn drop(&mut self) {
        let exited = CURRENT.with(|current| current.replace(self.previous.take()));
        drop(exited);
    }
}

pub(crate) struct Restore<'a, T: Copy>(&'a Cell<T>, T);

impl<T: Copy> Drop for Restore<'_, T> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}

pub(crate) fn scoped<T: Copy>(cell: &Cell<T>, value: T) -> Restore<'_, T> {
    Restore(cell, cell.replace(value))
}

impl Graph {
    pub(crate) fn create_node(
        &self,
        owner: Option<NodeId>,
        value: Option<Value>,
        compute: Option<Compute>,
    ) -> NodeId {
        let mut nodes = self.nodes.borrow_mut();
        let owner = owner.filter(|o| nodes.contains_key(*o));
        let state = if compute.is_some() {
            State::Dirty
        } else {
            State::Clean
        };
        let id = nodes.insert(Node {
            state,
            value,
            compute,
            owner,
            ..Default::default()
        });
        if let Some(owner) = owner {
            nodes[owner].children.insert(id);
        }
        id
    }

    pub(crate) fn is_alive(&self, id: NodeId) -> bool {
        self.nodes.borrow().contains_key(id)
    }

    pub(crate) fn current_owner(&self) -> Option<NodeId> {
        self.owner.get()
    }

    pub(crate) fn value(&self, id: NodeId) -> Option<Value> {
        self.nodes.borrow().get(id).and_then(|n| n.value.clone())
    }

    fn state(&self, id: NodeId) -> Option<State> {
        self.nodes.borrow().get(id).map(|n| n.state)
    }

    fn cleanup(&self, id: NodeId) {
        let (children, cleanups) = {
            let mut nodes = self.nodes.borrow_mut();
            let Some(node) = nodes.get_mut(id) else {
                return;
            };
            (take(&mut node.children), take(&mut node.cleanups))
        };
        for child in children.into_iter().rev() {
            self.dispose_inner(child, false);
        }
        for cleanup in cleanups.into_iter().rev() {
            cleanup();
        }
    }

    pub(crate) fn dispose(&self, id: NodeId) {
        self.dispose_inner(id, true);
    }

    fn dispose_inner(&self, id: NodeId, detach_from_owner: bool) {
        self.cleanup(id);
        let node = {
            let mut nodes = self.nodes.borrow_mut();
            let Some(node) = nodes.remove(id) else { return };
            for source in node.sources.iter().chain(&node.previous) {
                if let Some(s) = nodes.get_mut(*source) {
                    s.subscribers.swap_remove(&id);
                }
            }
            for sub in &node.subscribers {
                if let Some(s) = nodes.get_mut(*sub) {
                    s.sources.retain(|x| *x != id);
                }
            }
            if detach_from_owner && let Some(owner) = node.owner.and_then(|o| nodes.get_mut(o)) {
                owner.children.swap_remove(&id);
            }
            node
        };
        drop(node);
    }

    fn teardown(&self) {
        let roots: Vec<_> = {
            let nodes = self.nodes.borrow();
            nodes
                .iter()
                .filter(|(_, node)| node.owner.is_none())
                .map(|(id, _)| id)
                .collect()
        };
        for root in roots.into_iter().rev() {
            self.dispose(root);
        }
        self.clear_tasks();
        let rest = take(&mut *self.nodes.borrow_mut());
        drop(rest);
    }

    pub(crate) fn add_cleanup(&self, f: Box<dyn FnOnce()>) {
        if let Some(owner) = self.owner.get()
            && let Some(node) = self.nodes.borrow_mut().get_mut(owner)
        {
            node.cleanups.push(f);
        }
    }

    pub(crate) fn track(&self, source: NodeId) {
        let Some(observer) = self.observer.get() else {
            return;
        };
        if observer == source {
            return;
        }
        let mut nodes = self.nodes.borrow_mut();
        if !nodes.contains_key(source) {
            return;
        }
        let Some(obs) = nodes.get_mut(observer) else {
            return;
        };
        if obs.sources.last() == Some(&source) || obs.sources.contains(&source) {
            return;
        }
        obs.sources.push(source);
        nodes[source].subscribers.insert(observer);
    }

    pub(crate) fn with_owner<T>(&self, owner: Option<NodeId>, f: impl FnOnce() -> T) -> T {
        let _observer = scoped(&self.observer, None);
        let _owner = scoped(&self.owner, owner);
        f()
    }

    pub(crate) fn notify(&self, source: NodeId) {
        self.mark_subscribers(source);
        self.flush();
    }

    fn mark_subscribers(&self, source: NodeId) {
        let mut nodes = self.nodes.borrow_mut();
        let Some(node) = nodes.get(source) else {
            return;
        };
        let mut pending = self.pending_effects.borrow_mut();
        let mut stack: Vec<_> = node
            .subscribers
            .iter()
            .map(|s| (*s, State::Dirty, source))
            .collect();
        while let Some((id, state, from)) = stack.pop() {
            let Some(node) = nodes.get_mut(id) else {
                continue;
            };
            if node.state >= state || (node.computing && !node.sources.contains(&from)) {
                continue;
            }
            if node.state == State::Clean && node.value.is_none() {
                pending.push(id);
            }
            node.state = state;
            stack.extend(node.subscribers.iter().map(|s| (*s, State::Check, id)));
        }
    }

    pub(crate) fn update_if_necessary(&self, id: NodeId) {
        if self.state(id) == Some(State::Check) {
            let sources = match self.nodes.borrow().get(id) {
                Some(node) => node.sources.clone(),
                None => return,
            };
            for source in sources {
                self.update_if_necessary(source);
                if self.state(id) != Some(State::Check) {
                    break;
                }
            }
            if let Some(node) = self.nodes.borrow_mut().get_mut(id)
                && node.state == State::Check
            {
                node.state = State::Clean;
            }
        }
        if self.state(id) == Some(State::Dirty) {
            self.run_computation(id);
        }
    }

    fn run_computation(&self, id: NodeId) {
        self.cleanup(id);
        let compute = {
            let mut nodes = self.nodes.borrow_mut();
            let Some(node) = nodes.get_mut(id) else {
                return;
            };
            node.state = State::Clean;
            let mut previous = take(&mut node.sources);
            previous.append(&mut node.previous);
            node.previous = previous;
            node.computing = true;
            node.compute.clone()
        };
        let Some(compute) = compute else {
            return;
        };
        let changed = {
            let _batch = scoped(&self.batch_depth, self.batch_depth.get() + 1);
            let _observer = scoped(&self.observer, Some(id));
            let _owner = scoped(&self.owner, Some(id));
            compute()
        };
        {
            let mut nodes = self.nodes.borrow_mut();
            let Some(node) = nodes.get_mut(id) else {
                return;
            };
            node.computing = false;
            let previous = take(&mut node.previous);
            let dropped: Vec<NodeId> = previous
                .into_iter()
                .filter(|source| !node.sources.contains(source))
                .collect();
            for source in dropped {
                if let Some(s) = nodes.get_mut(source) {
                    s.subscribers.swap_remove(&id);
                }
            }
        }
        if changed {
            self.mark_subscribers(id);
        }
        self.flush();
    }

    pub(crate) fn batch<T>(&self, f: impl FnOnce() -> T) -> T {
        let result = {
            let _batch = scoped(&self.batch_depth, self.batch_depth.get() + 1);
            f()
        };
        self.flush();
        result
    }

    fn flush(&self) {
        if self.flushing.get() || self.batch_depth.get() > 0 {
            return;
        }
        let _flushing = scoped(&self.flushing, true);
        for round in 0.. {
            let queue = take(&mut *self.pending_effects.borrow_mut());
            if queue.is_empty() {
                break;
            }
            if round >= MAX_FLUSH_ROUNDS {
                debug_assert!(
                    false,
                    "rouge-reactive: effects did not settle after {MAX_FLUSH_ROUNDS} rounds; \
                     an effect is writing to a signal that (directly or indirectly) re-triggers it"
                );
                break;
            }
            for id in queue {
                self.update_if_necessary(id);
            }
        }
    }

    pub(crate) fn provide_context(&self, type_id: TypeId, value: Rc<dyn Any>) -> bool {
        let Some(owner) = self.owner.get() else {
            return false;
        };
        let mut nodes = self.nodes.borrow_mut();
        let Some(node) = nodes.get_mut(owner) else {
            return false;
        };
        node.contexts.insert(type_id, value);
        true
    }

    pub(crate) fn use_context(&self, type_id: TypeId) -> Option<Rc<dyn Any>> {
        let nodes = self.nodes.borrow();
        let mut current = self.owner.get();
        while let Some(id) = current {
            let node = nodes.get(id)?;
            if let Some(value) = node.contexts.get(&type_id) {
                return Some(value.clone());
            }
            current = node.owner;
        }
        None
    }
}
