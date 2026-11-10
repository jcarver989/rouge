use crate::tree::{ElementId, Presence};
use crate::view::{AnyView, BuildCx, View, send};
use rouge_reactive::{ReadSignal, Scope, Signal, effect, memo, on_cleanup, signal, untrack};
use rouge_render::{Pose, Timing};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;

type ViewFn = Box<dyn Fn() -> AnyView>;

pub fn dynamic<T: View>(f: impl Fn() -> T + 'static) -> impl View {
    Dynamic(Box::new(move || Some(f().into_any())), Presence::default())
}

struct Dynamic(Box<dyn Fn() -> Option<AnyView>>, Presence);

impl View for Dynamic {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let anchor = cx.tree.create_anchor();
        let patches = cx.patches.clone();
        let Dynamic(view, presence) = self;
        effect(move || {
            let Some(view) = view() else { return };
            let scope = Scope::new();
            send(&patches, move |cx| {
                if scope.is_alive() {
                    let element = scope.run(|| view.build(cx));
                    cx.tree.present(element, presence);
                    cx.tree.set_followers(anchor, vec![element]);
                }
            });
            let patches = patches.clone();
            on_cleanup(move || {
                send(&patches, move |cx| {
                    cx.tree.clear_followers(anchor, presence.exit);
                });
            });
        });
        anchor
    }
}

pub fn overlay<T: View>(view: impl FnOnce() -> T + 'static) -> impl View {
    Overlay(Box::new(move || view().into_any()))
}

struct Overlay(Box<dyn FnOnce() -> AnyView>);

impl View for Overlay {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let anchor = cx.tree.create_anchor();
        let element = (self.0)().build(cx);
        let root = cx.tree.overlay_root();
        cx.tree.append_child(root, element);
        let patches = cx.patches.clone();
        on_cleanup(move || send(&patches, move |cx| cx.tree.remove(element, None)));
        anchor
    }
}

pub struct Show {
    when: Box<dyn Fn() -> bool>,
    then: ViewFn,
    otherwise: Option<ViewFn>,
    presence: Presence,
}

pub fn show<T: View>(when: impl Fn() -> bool + 'static, view: impl Fn() -> T + 'static) -> Show {
    Show {
        when: Box::new(when),
        then: Box::new(move || view().into_any()),
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
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
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
        Dynamic(Box::new(branch), presence).build(cx)
    }
}

pub struct Each<T, U> {
    items: Box<dyn Fn() -> Vec<T>>,
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

pub fn each<T, U, V>(
    items: impl Fn() -> Vec<T> + 'static,
    key: impl Fn(&T) -> U + 'static,
    view: impl Fn(ReadSignal<T>) -> V + 'static,
) -> Each<T, U>
where
    T: PartialEq + 'static,
    U: Eq + Hash + Clone + 'static,
    V: View,
{
    Each {
        items: Box::new(items),
        key: Box::new(key),
        view: Box::new(move |item| view(item).into_any()),
        presence: Presence::default(),
    }
}

struct Row<T> {
    scope: Scope,
    item: Signal<T>,
    element: ElementId,
}

impl<T, U> View for Each<T, U>
where
    T: PartialEq + 'static,
    U: Eq + Hash + Clone + 'static,
{
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let anchor = cx.tree.create_anchor();
        let patches = cx.patches.clone();
        let Each {
            items,
            key,
            view,
            presence,
        } = self;
        let rows_scope = Scope::new();
        let state = Rc::new((key, view, RefCell::new(HashMap::<U, Row<T>>::new())));
        let cleanup = patches.clone();
        on_cleanup(move || {
            send(&cleanup, move |cx| {
                cx.tree.clear_followers(anchor, presence.exit);
            });
        });

        effect(move || {
            let (items, state) = (items(), state.clone());
            send(&patches, move |cx| {
                if !rows_scope.is_alive() {
                    return;
                }
                let (key, view, rows) = &*state;
                let mut rows = rows.borrow_mut();
                let mut seen = HashSet::with_capacity(items.len());
                let mut order = Vec::with_capacity(items.len());
                for item in items {
                    let k = key(&item);
                    if !seen.insert(k.clone()) {
                        debug_assert!(
                            false,
                            "rouge: duplicate key in `each`; the item was skipped"
                        );
                        continue;
                    }
                    if let Some(row) = rows.get(&k) {
                        row.item.set_if_changed(item);
                        order.push(row.element);
                    } else {
                        let scope = rows_scope.child();
                        let (item, element) = scope.run(|| {
                            let item = signal(item);
                            (item, view(item.read_only()).build(cx))
                        });
                        cx.tree.present(element, presence);
                        order.push(element);
                        rows.insert(
                            k,
                            Row {
                                scope,
                                item,
                                element,
                            },
                        );
                    }
                }
                rows.retain(|k, row| {
                    let keep = seen.contains(k);
                    if !keep {
                        cx.tree.remove(row.element, presence.exit);
                        row.scope.dispose();
                    }
                    keep
                });
                cx.tree.set_followers(anchor, order);
            });
        });
        anchor
    }
}
