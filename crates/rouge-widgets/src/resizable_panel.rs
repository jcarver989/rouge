use crate::components::{IconButton, icon_button};
use crate::icon::IconName;
use crate::material::{Material, MaterialExt};
use crate::popover::{FADED, dismiss_layer};
use crate::snap::dragged;
use crate::tokens::{Pace, Space};
use rouge_core::Role;
use rouge_core::component;
use rouge_core::{
    BuildCx, Cursor, DelegateStyle, Div, DragAxis, DragPhase, Fill, IntoProp, Key, Length,
    MotionProp, PointerEvent, Prop, Styled, Theme, View, div, motion, overlay, presentation,
    safe_area, show, view, window_size,
};
use rouge_reactive::{Memo, Signal, effect, memo, signal};
use std::cell::Cell;
use std::rc::Rc;

const HANDLE_SIZE: f32 = 6.0;
const DIVIDER_SIZE: f32 = 1.0;
const ACTIVE_DIVIDER_SIZE: f32 = 4.0;
const ACTIVE_DIVIDER_ALPHA: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

pub struct ResizablePanel {
    root: Div,
    side: Side,
    size: Signal<f32>,
    min: f32,
    max: f32,
    reset: f32,
    open: Option<Signal<bool>>,
    divider_color: Option<Prop<Fill>>,
    on_reset: Option<Rc<dyn Fn()>>,
    on_resize: Option<Rc<dyn Fn(f32)>>,
}

#[component]
pub fn resizable_panel(edge: Side, size: Signal<f32>) -> ResizablePanel {
    ResizablePanel {
        root: div().relative().flex_none(),
        side: edge,
        size,
        min: 0.0,
        max: f32::MAX,
        reset: size.get_untracked(),
        open: None,
        divider_color: None,
        on_reset: None,
        on_resize: None,
    }
}

impl ResizablePanel {
    pub fn min_size(mut self, size: f32) -> Self {
        self.min = size.max(0.0);
        self
    }

    pub fn max_size(mut self, size: f32) -> Self {
        self.max = size.max(0.0);
        self
    }

    pub fn reset_size(mut self, size: f32) -> Self {
        self.reset = size;
        self
    }

    pub fn divider_color<T>(mut self, color: impl IntoProp<Fill, T>) -> Self {
        self.divider_color = Some(color.into_prop());
        self
    }

    pub fn on_resize(mut self, f: impl Fn(f32) + 'static) -> Self {
        self.on_resize = Some(Rc::new(f));
        self
    }

    pub fn on_reset(mut self, f: impl Fn() + 'static) -> Self {
        self.on_reset = Some(Rc::new(f));
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.root = self.root.test_id(id.into());
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.root = self.root.child(child);
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.root = self.root.children(children);
        self
    }

    pub fn open(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }
}

impl DelegateStyle for ResizablePanel {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.root
    }
}

impl View for ResizablePanel {
    fn build(self, cx: &mut BuildCx<'_>) {
        let Self {
            mut root,
            side,
            size,
            min,
            max,
            reset,
            open,
            divider_color,
            on_reset,
            on_resize,
        } = self;
        let sideways = matches!(side, Side::Left | Side::Right);
        if let Some(open) =
            open.filter(|_| sideways && presentation().get_untracked().is_handheld())
        {
            return drawer(root, side, size, open, cx);
        }
        let shown = move || open.is_none_or(|open| open.get());
        let max = max.max(min);
        effect(move || {
            size.set_if_changed(size.get().clamp(min, max));
        });
        let set = Rc::new(move |next: f32| {
            let next = next.clamp(min, max);
            if size.get_untracked() != next {
                size.set(next);
                if let Some(on_resize) = &on_resize {
                    on_resize(next);
                }
            }
        });
        let reset_to = set.clone();
        let origin = Cell::new(0.0);
        let (hovered, dragging) = (signal(false), signal(false));
        let engaged = memo(move || hovered.get() || dragging.get());
        let handle = div()
            .absolute()
            .test_id("panel-resize-handle")
            .preserve_focus()
            .on_pointer_down(move |event| {
                if event.click_count == 2 {
                    reset_to(reset);
                    if let Some(reset) = &on_reset {
                        reset();
                    }
                }
                event.stop_propagation();
            })
            .on_hover(move |inside| hovered.set(inside))
            .on_drag(move |event| {
                match event.phase {
                    DragPhase::Start => {
                        origin.set(size.get_untracked());
                        dragging.set(true);
                    }
                    DragPhase::Move => {}
                    DragPhase::End => dragging.set(false),
                }
                let delta = match side {
                    Side::Left => event.translation.x,
                    Side::Right => -event.translation.x,
                    Side::Top => event.translation.y,
                    Side::Bottom => -event.translation.y,
                };
                set(origin.get() + delta);
            });
        let extent = move || if shown() { size.get() } else { 0.0 };
        if open.is_some() {
            root = root
                .overflow_hidden()
                .when(move || !shown(), Styled::invisible);
        }
        let cursor = if sideways {
            root = root.w(extent);
            Cursor::ResizeHorizontal
        } else {
            root = root.h(extent);
            Cursor::ResizeVertical
        };
        let handle = thickness(pin_to_edge(handle, side), side, HANDLE_SIZE)
            .cursor(cursor)
            .child(divider(side, divider_color, engaged));
        root.child(handle).build(cx);
    }
}

fn drawer(root: Div, edge: Side, extent: Signal<f32>, open: Signal<bool>, cx: &mut BuildCx<'_>) {
    let (window, safe) = (window_size(), safe_area());
    let width = move || extent.get().min(window.get().width - DRAWER_PEEK).max(0.0);
    let away = if edge == Side::Left { -1.0 } else { 1.0 };
    let closed = move || away * (width() + DRAWER_INSET);
    let x = motion(if open.get_untracked() { 0.0 } else { closed() });
    effect(move || {
        if open.get() {
            x.animate_to(0.0, Pace::Gesture);
        } else {
            x.animate_to(closed(), Pace::Exit);
        }
    });
    let dim = move || match closed() {
        0.0 => MotionProp::Value(0.0),
        closed => MotionProp::Mapped(x.map([closed, 0.0], [0.0, 1.0])),
    };
    let scrim = overlay(move || {
        let layer = move || {
            dismiss_layer(div(), move || open.set(false)).child(view! {
                <div test_id="drawer-scrim" absolute inset=0.0 pass_through bg=Role::Scrim opacity=dim/>
            })
        };
        view! { <show when={move || open.get()} children=layer exit=(FADED, Pace::Exit)/> }
    });
    let panel = view! {
        <div test_id="drawer" absolute top={move || DRAWER_INSET + safe.get().top}
             bottom={move || DRAWER_INSET + safe.get().bottom} w=width flex col p=Space::X1 overflow_hidden
             material=Material::Glass translate_x=x on_pointer_down=PointerEvent::stop_propagation
             on_key_down={move |event| {
                 if event.key == Key::Escape {
                     open.set(false);
                     event.stop_propagation();
                 }
             }}>
            {root.w_full().flex_1().min_h(0.0)}
        </div>
    };
    let panel = dragged(
        panel,
        DragAxis::Horizontal,
        x,
        move || (closed().min(0.0), closed().max(0.0)),
        move |projected, _| {
            if projected * away > width() / 2.0 {
                open.set(false);
                None
            } else {
                Some(0.0)
            }
        },
    );
    let panel = if edge == Side::Left {
        panel.left(DRAWER_INSET)
    } else {
        panel.right(DRAWER_INSET)
    };
    scrim.build(cx);
    overlay(move || panel).build(cx);
}

const DRAWER_INSET: f32 = 8.0;
const DRAWER_PEEK: f32 = 56.0;

#[component]
pub fn panel_toggle(side: Side, open: Signal<bool>, name: &str) -> IconButton {
    let icon = match side {
        Side::Left => IconName::PanelLeft,
        Side::Right => IconName::PanelRight,
        Side::Top | Side::Bottom => IconName::Rows2,
    };
    icon_button(icon, format!("Toggle {name}"))
        .selected(open)
        .on_click(move || open.toggle())
}

fn divider(side: Side, color: Option<Prop<Fill>>, engaged: Memo<bool>) -> Div {
    let theme = Theme::current();
    let resting = color.unwrap_or_else(|| Role::Border.into_prop());
    let line = pin_to_edge(div().absolute(), side).border_color(move || {
        if engaged.get() {
            Fill::Color(theme.get().accent.with_alpha(ACTIVE_DIVIDER_ALPHA))
        } else {
            resting.get()
        }
    });
    let weight = move || {
        if engaged.get() {
            ACTIVE_DIVIDER_SIZE
        } else {
            DIVIDER_SIZE
        }
    };
    match side {
        Side::Left | Side::Right => line.border_l(weight),
        Side::Top | Side::Bottom => line.border_t(weight),
    }
}

fn pin_to_edge(element: Div, side: Side) -> Div {
    match side {
        Side::Left => element.top(0.0).bottom(0.0).right(0.0),
        Side::Right => element.top(0.0).bottom(0.0).left(0.0),
        Side::Top => element.left(0.0).right(0.0).bottom(0.0),
        Side::Bottom => element.left(0.0).right(0.0).top(0.0),
    }
}

fn thickness<T>(element: Div, side: Side, extent: impl IntoProp<Length, T>) -> Div {
    match side {
        Side::Left | Side::Right => element.w(extent),
        Side::Top | Side::Bottom => element.h(extent),
    }
}
