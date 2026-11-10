use rouge_core::{BuildCx, Cursor, Div, DragPhase, ElementId, Styled, View, div, styled};
use rouge_reactive::{Signal, effect};
use std::cell::Cell;
use std::rc::Rc;

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
    on_resize: Option<Rc<dyn Fn(f32)>>,
}

pub fn resizable_panel(edge: Side, size: Signal<f32>) -> ResizablePanel {
    ResizablePanel {
        root: div().relative().flex_none(),
        side: edge,
        size,
        min: 0.0,
        max: f32::MAX,
        reset: size.get_untracked(),
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
    pub fn on_resize(mut self, f: impl Fn(f32) + 'static) -> Self {
        self.on_resize = Some(Rc::new(f));
        self
    }
    pub fn child(mut self, child: impl View) -> Self {
        self.root = self.root.child(child);
        self
    }
}

styled!(ResizablePanel => root);

impl View for ResizablePanel {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let Self {
            mut root,
            side,
            size,
            min,
            max,
            reset,
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
        let handle = div()
            .absolute()
            .test_id("panel-resize-handle")
            .preserve_focus()
            .on_pointer_down(move |event| {
                if event.click_count == 2 {
                    reset_to(reset);
                }
                event.stop_propagation();
            })
            .on_drag(move |event| {
                if event.phase == DragPhase::Start {
                    origin.set(size.get_untracked());
                }
                let delta = match side {
                    Side::Left => event.translation.x,
                    Side::Right => -event.translation.x,
                    Side::Top => event.translation.y,
                    Side::Bottom => -event.translation.y,
                };
                set(origin.get() + delta);
            });
        let handle = match side {
            Side::Left | Side::Right => {
                root = root.w(size);
                let handle = handle
                    .top(0.0)
                    .bottom(0.0)
                    .w(6.0)
                    .cursor(Cursor::ResizeHorizontal);
                if side == Side::Left {
                    handle.right(0.0)
                } else {
                    handle.left(0.0)
                }
            }
            Side::Top | Side::Bottom => {
                root = root.h(size);
                let handle = handle
                    .left(0.0)
                    .right(0.0)
                    .h(6.0)
                    .cursor(Cursor::ResizeVertical);
                if side == Side::Top {
                    handle.bottom(0.0)
                } else {
                    handle.top(0.0)
                }
            }
        };
        root.child(handle).build(cx)
    }
}
