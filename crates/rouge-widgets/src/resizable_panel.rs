use crate::components::{IconButton, icon_button};
use crate::icon::IconName;
use rouge_core::Role;
use rouge_core::component;
use rouge_core::{
    BuildCx, Cursor, DelegateStyle, Div, DragPhase, Fill, IntoProp, Length, Prop, Styled, Theme,
    View, div,
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
        self.root = self.root.test_id(id);
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
            divider_color,
            on_reset,
            on_resize,
        } = self;
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
        let cursor = match side {
            Side::Left | Side::Right => {
                root = root.w(size);
                Cursor::ResizeHorizontal
            }
            Side::Top | Side::Bottom => {
                root = root.h(size);
                Cursor::ResizeVertical
            }
        };
        let handle = thickness(pin_to_edge(handle, side), side, HANDLE_SIZE)
            .cursor(cursor)
            .child(divider(side, divider_color, engaged));
        root.child(handle).build(cx);
    }
}

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
