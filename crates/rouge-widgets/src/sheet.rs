use crate::material::{Material, MaterialExt};
use crate::popover::dismiss_layer;
use crate::tokens::{Pace, Radius};
use crate::util::{TestId, focus_ring};
use rouge_core::view;
use rouge_core::{
    AnyView, BuildCx, Color, Cursor, DragAxis, DragPhase, Key, MotionProp, PointerEvent, Role,
    Size, Styled, Timing, View, component, div, motion, overlay, safe_area, show, window_size,
};
use rouge_reactive::{Callback, Signal, effect, signal, spawn};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Detent {
    #[default]
    Content,
    Medium,
    Large,
}

pub struct Sheet {
    presenter: Presenter,
    content: Rc<dyn Fn() -> AnyView>,
}

#[component]
pub fn sheet<T: View>(open: Signal<bool>, children: impl Fn() -> T + 'static) -> Sheet {
    Sheet {
        presenter: Presenter::new(open),
        content: Rc::new(move || children().into_any()),
    }
}

impl Sheet {
    pub fn detents(mut self, detents: impl IntoIterator<Item = Detent>) -> Self {
        self.presenter.detents = detents.into_iter().collect();
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.presenter.test_id = Some(id.into());
        self
    }
}

impl View for Sheet {
    fn build(self, cx: &mut BuildCx<'_>) {
        let content = self.content;
        self.presenter.present(move || content()).build(cx);
    }
}

pub(crate) struct Presenter {
    pub(crate) open: Signal<bool>,
    pub(crate) detents: Vec<Detent>,
    pub(crate) test_id: Option<String>,
    pub(crate) on_closed: Callback,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Closed,
    Measuring,
    Open(usize),
    Leaving,
}

impl Presenter {
    pub(crate) fn new(open: Signal<bool>) -> Self {
        Presenter {
            open,
            detents: Vec::new(),
            test_id: None,
            on_closed: Callback::default(),
        }
    }

    pub(crate) fn present(self, content: impl Fn() -> AnyView + 'static) -> impl View {
        let Presenter {
            open,
            mut detents,
            test_id,
            on_closed,
        } = self;
        if detents.is_empty() {
            detents.push(Detent::default());
        }
        let detents: Rc<[Detent]> = detents.into();
        let fitted = detents.iter().all(|detent| *detent == Detent::Content);
        let (window, safe) = (window_size(), safe_area());
        let (panel, body) = (signal(Size::ZERO), signal(Size::ZERO));
        let (y, origin) = (motion(0.0), signal(0.0));
        let (shown, phase) = (signal(false), signal(Phase::Closed));
        let room = move || (window.get().height - safe.get().top - TOP_GAP).max(0.0);
        let height = {
            let detents = detents.clone();
            move |index: usize| match detents.get(index).copied().unwrap_or_default() {
                Detent::Content if fitted => (panel.get().height - OVERHANG).max(0.0),
                Detent::Content => (body.get().height + HANDLE).min(room()),
                Detent::Medium => (window.get().height * MEDIUM).min(room()),
                Detent::Large => room(),
            }
        };
        let count = detents.len();
        let full = {
            let height = height.clone();
            move || (0..count).map(&height).fold(0.0, f32::max)
        };
        let rest = {
            let (height, full) = (height.clone(), full.clone());
            move |index: usize| (full() - height(index)).max(0.0)
        };
        effect({
            let (full, rest) = (full.clone(), rest.clone());
            move || {
                let (wanted, full) = (open.get(), full());
                let now = phase.get_untracked();
                if wanted {
                    shown.set_if_changed(true);
                    if full <= 0.0 {
                        if now != Phase::Measuring {
                            y.set(window.get_untracked().height);
                            phase.set(Phase::Measuring);
                        }
                        return;
                    }
                    let index = match now {
                        Phase::Open(index) => index,
                        Phase::Closed | Phase::Measuring => {
                            y.set(full);
                            0
                        }
                        Phase::Leaving => 0,
                    };
                    phase.set(Phase::Open(index));
                    y.animate_to(rest(index), Pace::Gesture);
                } else if matches!(now, Phase::Open(_) | Phase::Measuring) {
                    phase.set(Phase::Leaving);
                    let left = y.animate_to(full.max(y.get()), Pace::Exit);
                    spawn(async move {
                        if left.await && phase.get_untracked() == Phase::Leaving {
                            phase.set(Phase::Closed);
                            shown.set(false);
                            on_closed.call();
                        }
                    });
                }
            }
        });
        let settle = {
            let (full, rest) = (full.clone(), rest.clone());
            move |velocity: f32| {
                let projected = y.get() + velocity * PROJECTION;
                let nearest = (0..count)
                    .map(|index| (Some(index), rest(index)))
                    .chain([(None, full())])
                    .min_by(|a, b| (a.1 - projected).abs().total_cmp(&(b.1 - projected).abs()));
                match nearest {
                    Some((Some(index), at)) => {
                        phase.set(Phase::Open(index));
                        y.fling_to(at, velocity, Timing::from(Pace::Gesture));
                    }
                    _ => open.set(false),
                }
            }
        };
        let step = {
            let rest = rest.clone();
            move |up: bool| {
                let Phase::Open(index) = phase.get_untracked() else {
                    return;
                };
                let here = rest(index);
                let next = (0..count)
                    .map(|index| (index, rest(index)))
                    .filter(|(_, at)| if up { *at < here } else { *at > here })
                    .min_by(|a, b| (a.1 - here).abs().total_cmp(&(b.1 - here).abs()));
                match next {
                    Some((index, at)) => {
                        phase.set(Phase::Open(index));
                        y.animate_to(at, Pace::Gesture);
                    }
                    None if !up => open.set(false),
                    None => {}
                }
            }
        };
        let content = Rc::new(content);
        view! {
            <overlay>
                <show when={move || shown.get()}>
                    {
                        let (full, settle, step) = (full.clone(), settle.clone(), step.clone());
                        let dim = {
                            let full = full.clone();
                            move || {
                                let full = full();
                                if full > 0.0 {
                                    MotionProp::Mapped(y.map([0.0, full], [1.0, 0.0]))
                                } else {
                                    MotionProp::Value(0.0)
                                }
                            }
                        };
                        let scrim = view! {
                            <div absolute inset=0.0 pass_through bg=Role::Scrim opacity=dim test_id?=test_id.part("scrim")/>
                        };
                        let grabber = focus_ring(
                            view! {
                                <div flex flex_none items_center justify_center h=HANDLE border=1.0
                                     border_color=Color::TRANSPARENT rounded=Radius::Floating cursor=Cursor::Grab focusable
                                     on_key_down={move |event| {
                                         match event.key {
                                             Key::ArrowUp => step(true),
                                             Key::ArrowDown => step(false),
                                             _ => return,
                                         }
                                         event.stop_propagation();
                                     }}
                                     test_id?=test_id.part("grabber")>
                                    <div w=GRABBER_WIDTH h=GRABBER_HEIGHT rounded=Radius::Full bg=Role::Border/>
                                </div>
                            },
                        );
                        let inner = view! { <div flex col track_size=body>{content()}</div> };
                        let inner = if fitted {
                            inner
                        } else {
                            inner.flex_1().min_h(0.0)
                        };
                        let sized = {
                            let full = full.clone();
                            move || full() + OVERHANG
                        };
                        let surface = view! {
                            <div absolute left=0.0 right=0.0 bottom={-OVERHANG} flex col
                                 pb={move || OVERHANG + safe.get().bottom} max_h={move || room() + OVERHANG} overflow_hidden
                                 material=Material::Glass focus_scope autofocus track_size=panel translate_y=y
                                 on_pointer_down=PointerEvent::stop_propagation drag_axis=DragAxis::Vertical
                                 on_drag={move |event| match event.phase {
                                     DragPhase::Start => origin.set(y.get()),
                                     DragPhase::Move => {
                                         let to = origin.get_untracked() + event.translation.y;
                                         y.set(if to < 0.0 { to * RESISTANCE } else { to });
                                     }
                                     DragPhase::End => settle(event.velocity.y),
                                 }}
                                 test_id?=test_id.clone()>
                                {grabber}
                                {inner}
                            </div>
                        };
                        let surface = if fitted { surface } else { surface.h(sized) };
                        dismiss_layer(view! { <div/> }, move || open.set(false))
                            .child(scrim)
                            .child(surface)
                    }
                </show>
            </overlay>
        }
    }
}

const TOP_GAP: f32 = 8.0;
const MEDIUM: f32 = 0.5;
const OVERHANG: f32 = 64.0;
const HANDLE: f32 = 20.0;
const GRABBER_WIDTH: f32 = 36.0;
const GRABBER_HEIGHT: f32 = 4.0;
const PROJECTION: f32 = 0.2;
const RESISTANCE: f32 = 0.3;
