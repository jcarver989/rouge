use crate::animation::{Pose, Timing};
use crate::prop::markers;
use crate::tree::{FragmentId, Node, Presence};
use crate::view::{AnyView, BuildCx, Queue, View};
use rouge_macros::component;
use rouge_reactive::{ReadSignal, Scope, Signal, effect, memo, on_cleanup, signal, untrack};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

type ViewFn = Box<dyn Fn() -> AnyView>;

#[component]
pub fn dynamic<T: View>(children: impl Fn() -> T + 'static) -> impl View {
    Dynamic(
        Box::new(move || Some(children().into_any())),
        Presence::default(),
    )
}

#[component]
pub fn switch<T, U>(on: impl Fn() -> T + 'static, children: impl Fn(T) -> U + 'static) -> impl View
where
    T: Clone + PartialEq + 'static,
    U: View,
{
    let on = memo(on);
    dynamic(move || {
        let value = on.get();
        untrack(|| children(value))
    })
}

struct Dynamic(Box<dyn Fn() -> Option<AnyView>>, Presence);

impl View for Dynamic {
    fn build(self, cx: &mut BuildCx<'_>) {
        let fragment = cx.tree.fragment(cx.parent);
        let queue = cx.queue.clone();
        let Dynamic(view, presence) = self;
        remove_on_cleanup(&queue, fragment, presence);
        effect(move || {
            let Some(view) = view() else { return };
            let scope = Scope::new();
            queue.build(move |cx| {
                if scope.is_alive() {
                    cx.within(Node::Fragment(fragment), |cx| scope.run(|| view.build(cx)));
                    cx.tree.present(fragment, presence);
                }
            });
            let queue = queue.clone();
            on_cleanup(move || {
                queue.build(move |cx| cx.tree.clear(fragment, presence.exit));
            });
        });
    }
}

fn remove_on_cleanup(queue: &Queue, fragment: FragmentId, presence: Presence) {
    let queue = queue.clone();
    on_cleanup(move || {
        queue.build(move |cx| cx.tree.remove_fragment(fragment, presence.exit));
    });
}

#[component]
pub fn overlay<T: View>(children: impl FnOnce() -> T + 'static) -> impl View {
    Overlay(Box::new(move || children().into_any()))
}

struct Overlay(Box<dyn FnOnce() -> AnyView>);

impl View for Overlay {
    fn build(self, cx: &mut BuildCx<'_>) {
        let root = cx.tree.overlay_root();
        let fragment = cx.tree.fragment(Node::Element(root));
        cx.within(Node::Fragment(fragment), |cx| (self.0)().build(cx));
        remove_on_cleanup(cx.queue, fragment, Presence::default());
    }
}

pub struct Show {
    when: Box<dyn Fn() -> bool>,
    then: ViewFn,
    otherwise: Option<ViewFn>,
    presence: Presence,
}

#[component]
pub fn show<T: View>(
    when: impl Fn() -> bool + 'static,
    children: impl Fn() -> T + 'static,
) -> Show {
    Show {
        when: Box::new(when),
        then: Box::new(move || children().into_any()),
        otherwise: None,
        presence: Presence::default(),
    }
}

impl Show {
    pub fn otherwise<T: View>(mut self, view: impl Fn() -> T + 'static) -> Self {
        self.otherwise = Some(Box::new(move || view().into_any()));
        self
    }

    pub fn enter(mut self, from: Pose, timing: impl Into<Timing>) -> Self {
        self.presence.enter = Some((from, timing.into()));
        self
    }

    pub fn exit(mut self, to: Pose, timing: impl Into<Timing>) -> Self {
        self.presence.exit = Some((to, timing.into()));
        self
    }
}

impl View for Show {
    fn build(self, cx: &mut BuildCx<'_>) {
        let Show {
            when,
            then,
            otherwise,
            presence,
        } = self;
        let when = memo(when);
        let branch = move || {
            let view = if when.get() {
                Some(&then)
            } else {
                otherwise.as_ref()
            };
            view.map(untrack)
        };
        Dynamic(Box::new(branch), presence).build(cx);
    }
}

type ReadItems<T> = Box<dyn FnOnce(&mut dyn FnMut(&[T]))>;

pub trait IntoItems<T, U> {
    fn into_items(self) -> Box<dyn Fn() -> ReadItems<T>>;
}

impl<T: 'static, U: Fn() -> Vec<T> + 'static> IntoItems<T, markers::Func> for U {
    fn into_items(self) -> Box<dyn Fn() -> ReadItems<T>> {
        Box::new(move || {
            let items = self();
            Box::new(move |read: &mut dyn FnMut(&[T])| read(&items))
        })
    }
}

impl<T: 'static, U: AsRef<[T]> + 'static> IntoItems<T, markers::Reactive> for ReadSignal<U> {
    fn into_items(self) -> Box<dyn Fn() -> ReadItems<T>> {
        Box::new(move || {
            self.with(|_| ());
            Box::new(move |read: &mut dyn FnMut(&[T])| {
                self.with_untracked(|items| read(items.as_ref()));
            })
        })
    }
}

impl<T: 'static, U: AsRef<[T]> + 'static> IntoItems<T, markers::Reactive> for Signal<U> {
    fn into_items(self) -> Box<dyn Fn() -> ReadItems<T>> {
        self.read_only().into_items()
    }
}

pub struct Each<T, U> {
    items: Box<dyn Fn() -> ReadItems<T>>,
    key: Box<dyn Fn(&T) -> U>,
    view: Box<dyn Fn(ReadSignal<T>) -> AnyView>,
    presence: Presence,
}

impl<T, U> Each<T, U> {
    pub fn enter(mut self, from: Pose, timing: impl Into<Timing>) -> Self {
        self.presence.enter = Some((from, timing.into()));
        self
    }

    pub fn exit(mut self, to: Pose, timing: impl Into<Timing>) -> Self {
        self.presence.exit = Some((to, timing.into()));
        self
    }
}

#[component]
pub fn each<T, U, V, W>(
    items: impl IntoItems<T, W>,
    key: impl Fn(&T) -> U + 'static,
    children: impl Fn(ReadSignal<T>) -> V + 'static,
) -> Each<T, U>
where
    T: Clone + PartialEq + 'static,
    U: Eq + Hash + Clone + 'static,
    V: View,
{
    Each {
        items: items.into_items(),
        key: Box::new(key),
        view: Box::new(move |item| children(item).into_any()),
        presence: Presence::default(),
    }
}

struct Row<T> {
    scope: Scope,
    item: Signal<T>,
    fragment: FragmentId,
}

impl<T, U> View for Each<T, U>
where
    T: Clone + PartialEq + 'static,
    U: Eq + Hash + Clone + 'static,
{
    fn build(self, cx: &mut BuildCx<'_>) {
        let fragment = cx.tree.fragment(cx.parent);
        let queue = cx.queue.clone();
        let Each {
            items,
            key,
            view,
            presence,
        } = self;
        let rows_scope = Scope::new();
        let state = Rc::new((key, view, RefCell::new(FxHashMap::<U, Row<T>>::default())));
        remove_on_cleanup(&queue, fragment, presence);

        effect(move || {
            let (read, state) = (items(), state.clone());
            let pending = Scope::new();
            queue.build(move |cx| {
                if !pending.is_alive() || !rows_scope.is_alive() {
                    return;
                }
                let (key, view, rows) = &*state;
                let mut rows = rows.borrow_mut();
                let mut seen = FxHashSet::default();
                let mut order = Vec::new();
                read(&mut |items| {
                    seen.reserve(items.len());
                    order.reserve(items.len());
                    for item in items {
                        let k = key(item);
                        if !seen.insert(k.clone()) {
                            debug_assert!(
                                false,
                                "rouge: duplicate key in `each`; the item was skipped"
                            );
                            continue;
                        }
                        if let Some(row) = rows.get(&k) {
                            if row.item.with_untracked(|current| current != item) {
                                row.item.set(item.clone());
                            }
                            order.push(Node::Fragment(row.fragment));
                        } else {
                            let scope = rows_scope.child();
                            let row = cx.tree.detached_fragment();
                            let item = scope.run(|| {
                                let item = signal(item.clone());
                                let view = view(item.read_only());
                                cx.within(Node::Fragment(row), |cx| view.build(cx));
                                item
                            });
                            cx.tree.present(row, presence);
                            order.push(Node::Fragment(row));
                            rows.insert(
                                k,
                                Row {
                                    scope,
                                    item,
                                    fragment: row,
                                },
                            );
                        }
                    }
                });
                rows.retain(|k, row| {
                    let keep = seen.contains(k);
                    if !keep {
                        cx.tree.remove_fragment(row.fragment, presence.exit);
                        row.scope.dispose();
                    }
                    keep
                });
                cx.tree.reorder(fragment, order);
            });
        });
    }
}
