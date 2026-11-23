use crate::tokens::{Pace, Radius};
use crate::util::{chrome, focus_ring};
use rouge_core::access;
use rouge_core::component;
use rouge_core::view;
use rouge_core::{
    AnyView, BuildCx, Color, Cursor, DelegateStyle, DragPhase, Key, Size, StyleDelta, Styled, View,
    div, each, motion,
};
use rouge_reactive::{KeyedList, batch, signal};
use std::hash::Hash;
use std::rc::Rc;

pub struct Sortable<T: 'static, U: 'static> {
    list: KeyedList<T, U>,
    row: RowView<T, U>,
    test_id: Option<String>,
    style: StyleDelta,
}

type RowView<T, U> = Rc<dyn Fn(&T, &U) -> AnyView>;

#[component]
pub fn sortable<T, U, V>(
    list: KeyedList<T, U>,
    row: impl Fn(&T, &U) -> V + 'static,
) -> Sortable<T, U>
where
    T: Eq + Hash + Clone + 'static,
    V: View,
{
    Sortable {
        list,
        row: Rc::new(move |key, value| row(key, value).into_any()),
        test_id: None,
        style: StyleDelta::default(),
    }
}

chrome!(impl[T, U] Sortable<T, U>);

#[derive(Clone, PartialEq)]
struct Carry<T> {
    key: T,
    from: usize,
    to: usize,
    slot: f32,
}

impl<T: Eq + Hash + Clone + 'static, U: 'static> View for Sortable<T, U> {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (list, row) = (self.list, self.row);
        let carry = signal(None::<Carry<T>>);
        let rows = view! {
            <each items=list.keys() key=Clone::clone let:key>
                {
                    let key = key.get_untracked();
                    let (offset, size) = (motion(0.0), signal(Size::ZERO));
                    let place = {
                        let key = key.clone();
                        move || {
                            list.keys()
                                .with(|keys| keys.iter().position(|other| *other == key))
                                .unwrap_or_default()
                        }
                    };
                    let shift = {
                        let (key, place) = (key.clone(), place.clone());
                        move || {
                            carry.with(|carry| match carry {
                                Some(carry) if carry.key != key => {
                                    let at = place();
                                    if carry.from < at && at <= carry.to {
                                        -carry.slot
                                    } else if carry.to <= at && at < carry.from {
                                        carry.slot
                                    } else {
                                        0.0
                                    }
                                }
                                _ => 0.0,
                            })
                        }
                    };
                    let held = {
                        let key = key.clone();
                        move || carry.with(|carry| carry.as_ref().is_some_and(|carry| carry.key == key))
                    };
                    let content = list.with(&key, |value| row(&key, value));
                    let dragged = {
                        let (key, place) = (key.clone(), place.clone());
                        move |phase: DragPhase, travel: f32| match phase {
                            DragPhase::Start => {
                                let from = place();
                                carry.set(Some(Carry {
                                    key: key.clone(),
                                    from,
                                    to: from,
                                    slot: size.get_untracked().height,
                                }));
                            }
                            DragPhase::Move => {
                                offset.set(travel);
                                let end = list.len().saturating_sub(1);
                                carry.update(|carry| {
                                    if let Some(carry) = carry {
                                        let slots = (travel / carry.slot.max(1.0)).round();
                                        let to = carry.from.saturating_add_signed(slots as isize);
                                        carry.to = to.min(end);
                                    }
                                });
                            }
                            DragPhase::End => batch(|| {
                                if let Some(done) = carry.get_untracked() {
                                    list.move_to(&done.key, done.to);
                                }
                                carry.set(None);
                                offset.animate_to(0.0, Pace::Layout);
                            }),
                        }
                    };
                    let stepped = {
                        let (key, place) = (key.clone(), place.clone());
                        move |by: isize| {
                            list.move_to(&key, place().saturating_add_signed(by));
                        }
                    };
                    let body = view! {
                        <div translate_y=offset scale={move || if held() { LIFTED } else { 1.0 }} transition=Pace::Press>
                            {content}
                        </div>
                    };
                    let slot = view! {
                        <div role=access::Role::ListItem access_description="Alt and an arrow key moves this item"
                             border=1.0 border_color=Color::TRANSPARENT rounded=Radius::Inner focusable
                             cursor=Cursor::Grab track_size=size translate_y=shift transition=Pace::Layout
                             layout_transition=Pace::Layout
                             on_drag={move |event| dragged(event.phase, event.translation.y)}
                             on_key_down={move |event| {
                                 if !event.modifiers.alt {
                                     return;
                                 }
                                 match event.key {
                                     Key::ArrowUp => stepped(-1),
                                     Key::ArrowDown => stepped(1),
                                     _ => return,
                                 }
                                 event.stop_propagation();
                             }}>
                            {body}
                        </div>
                    };
                    focus_ring(slot)
                }
            </each>
        };
        let root = view! { <div flex col test_id?=self.test_id>{rows}</div> };
        self.style.apply(root).build(cx);
    }
}

const LIFTED: f32 = 1.02;
