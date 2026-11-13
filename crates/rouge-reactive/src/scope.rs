use crate::handle::Handle;
use crate::runtime::{Graph, NodeId, with_rt};
use std::any::{TypeId, type_name};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Scope {
    handle: Handle,
}

impl Scope {
    #[track_caller]
    pub fn new() -> Scope {
        Scope::create(with_rt(Graph::current_owner))
    }

    #[track_caller]
    pub fn root() -> Scope {
        Scope::create(None)
    }

    #[track_caller]
    pub fn current() -> Option<Scope> {
        with_rt(Graph::current_owner).map(|id| Scope {
            handle: Handle::new(id),
        })
    }

    #[track_caller]
    pub fn child(&self) -> Scope {
        Scope::create(Some(self.handle.id))
    }

    #[track_caller]
    fn create(owner: Option<NodeId>) -> Scope {
        Scope {
            handle: Handle::new(with_rt(|rt| rt.create_node(owner, None, None))),
        }
    }

    #[track_caller]
    pub fn run<T>(&self, f: impl FnOnce() -> T) -> T {
        if !self.is_alive() {
            self.handle.panic_disposed("scope", "run code in");
        }
        with_rt(|rt| rt.with_owner(Some(self.handle.id), f))
    }

    pub fn try_run<T>(&self, f: impl FnOnce() -> T) -> Option<T> {
        self.is_alive()
            .then(|| with_rt(|rt| rt.with_owner(Some(self.handle.id), f)))
    }

    pub fn dispose(&self) {
        self.handle.dispose();
    }

    pub fn is_alive(&self) -> bool {
        self.handle.is_alive()
    }
}

impl Default for Scope {
    #[track_caller]
    fn default() -> Self {
        Scope::new()
    }
}

pub fn run_in<T>(owner: Option<Scope>, f: impl FnOnce() -> T) -> Option<T> {
    match owner {
        Some(owner) => owner.try_run(f),
        None => Some(untrack(f)),
    }
}

pub fn on_cleanup(f: impl FnOnce() + 'static) {
    with_rt(|rt| rt.add_cleanup(Box::new(f)));
}

pub fn untrack<T>(f: impl FnOnce() -> T) -> T {
    with_rt(|rt| rt.with_owner(rt.current_owner(), f))
}

pub fn batch<T>(f: impl FnOnce() -> T) -> T {
    with_rt(|rt| rt.batch(f))
}

#[track_caller]
pub fn provide_context<T: 'static>(value: T) {
    assert!(
        with_rt(|rt| rt.provide_context(TypeId::of::<T>(), Rc::new(value))),
        "rouge-reactive: provide_context::<{}>() called outside of a reactive scope",
        type_name::<T>()
    );
}

pub fn use_context<T: Clone + 'static>() -> Option<T> {
    with_rt(|rt| rt.use_context(TypeId::of::<T>()))?
        .downcast_ref::<T>()
        .cloned()
}

#[track_caller]
pub fn expect_context<T: Clone + 'static>() -> T {
    use_context::<T>().unwrap_or_else(|| {
        panic!(
            "rouge-reactive: no context of type {} was provided by an ancestor scope",
            type_name::<T>()
        )
    })
}
