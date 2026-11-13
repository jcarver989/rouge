use crate::material::{Material, MaterialExt};
use crate::typography::UiStyles;
use rouge_core::component;
use rouge_core::{
    AnyView, BuildCx, Div, ElementId, Key, PointerEvent, Pose, Rect, Styled, Theme, Tween, View,
    div, overlay, px, show, styled, text, window_size,
};
use rouge_reactive::{Signal, Task, on_cleanup, signal, sleep, spawn};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

const GAP: f32 = 4.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Placement {
    #[default]
    Below,
    Above,
    Right,
    Left,
}

impl Placement {
    pub(crate) fn place(self, panel: Div, anchor: Signal<Rect>) -> Div {
        let window = window_size();
        let measured = signal(Rect::ZERO);
        let position = move || {
            let anchor = anchor.get();
            let size = measured.get().size;
            let window = window.get();
            let right = anchor.max().x + GAP;
            let left = anchor.origin.x - size.width - GAP;
            let below = anchor.max().y + GAP;
            let above = anchor.origin.y - size.height - GAP;
            let (x, y) = match self {
                Self::Below if below + size.height > window.height && above >= 0.0 => {
                    (anchor.origin.x, above)
                }
                Self::Above if above < 0.0 && below + size.height <= window.height => {
                    (anchor.origin.x, below)
                }
                Self::Right if right + size.width > window.width && left >= 0.0 => {
                    (left, anchor.origin.y)
                }
                Self::Left if left < 0.0 && right + size.width <= window.width => {
                    (right, anchor.origin.y)
                }
                Self::Below => (anchor.origin.x, below),
                Self::Above => (anchor.origin.x, above),
                Self::Right => (right, anchor.origin.y),
                Self::Left => (left, anchor.origin.y),
            };
            (
                x.clamp(0.0, (window.width - size.width).max(0.0)),
                y.clamp(0.0, (window.height - size.height).max(0.0)),
            )
        };
        panel
            .absolute()
            .max_w(move || px(window.get().width))
            .max_h(move || px(window.get().height))
            .track_bounds(measured)
            .left(0.0)
            .top(0.0)
            .translate_x(move || position().0)
            .translate_y(move || position().1)
    }
}

pub(crate) fn dismissable(layer: Div, panel: Div, dismiss: impl Fn() + Clone + 'static) -> Div {
    let escape = dismiss.clone();
    layer
        .absolute()
        .inset(0.0)
        .on_pointer_down(move |_| dismiss())
        .on_key_down(move |event| {
            if event.key == Key::Escape {
                escape();
                event.stop_propagation();
            }
        })
        .child(panel.on_pointer_down(PointerEvent::stop_propagation))
}

pub(crate) fn popup(
    open: Signal<bool>,
    backdrop: impl Fn(Div) -> Div + 'static,
    content: impl Fn() -> Div + 'static,
) -> impl View {
    overlay(move || {
        let layer = move || dismissable(backdrop(div()), content(), move || open.set(false));
        show(move || open.get(), layer)
            .enter(Pose::new().opacity(0.0), Tween::ms(120.0))
            .exit(Pose::new().opacity(0.0), Tween::ms(100.0))
    })
}

fn anchored(target: impl View) -> (Div, Signal<Rect>) {
    let anchor = signal(Rect::ZERO);
    (div().flex_none().track_bounds(anchor).child(target), anchor)
}

pub struct Popover {
    root: Div,
    open: Signal<bool>,
    anchor: Signal<Rect>,
    content: Box<dyn Fn() -> AnyView>,
    placement: Placement,
    material: Material,
}

#[component]
pub fn popover<T: View, U: View>(
    open: Signal<bool>,
    trigger: T,
    children: impl Fn() -> U + 'static,
) -> Popover {
    let (root, anchor) = anchored(trigger);
    Popover {
        root,
        open,
        anchor,
        content: Box::new(move || children().into_any()),
        placement: Placement::default(),
        material: Material::default(),
    }
}

impl Popover {
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    pub fn material(mut self, material: Material) -> Self {
        self.material = material;
        self
    }
}

styled!(Popover => root);

impl View for Popover {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (anchor, placement, content, material) =
            (self.anchor, self.placement, self.content, self.material);
        let body = move || {
            let body = div()
                .material(material)
                .focus_scope()
                .autofocus()
                .child(content());
            placement.place(body, anchor)
        };
        self.root
            .child(popup(self.open, |layer| layer, body))
            .build(cx)
    }
}

pub struct Tooltip {
    root: Div,
    label: String,
    delay: Duration,
    anchor: Signal<Rect>,
}

#[component]
pub fn tooltip(children: impl View, label: impl Into<String>) -> Tooltip {
    children.tooltip(label)
}

pub trait Hint: View + Sized {
    fn tooltip(self, label: impl Into<String>) -> Tooltip {
        let (root, anchor) = anchored(self);
        Tooltip {
            root,
            label: label.into(),
            delay: Duration::from_millis(500),
            anchor,
        }
    }

    fn hint(self, label: Option<impl Into<String>>) -> AnyView {
        match label {
            Some(label) => self.tooltip(label).into_any(),
            None => self.into_any(),
        }
    }
}

impl<T: View> Hint for T {}

impl Tooltip {
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

styled!(Tooltip => root);

impl View for Tooltip {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let theme = Theme::current();
        let (label, delay, anchor) = (self.label, self.delay, self.anchor);
        let open = signal(false);
        let pending = Rc::new(Cell::new(None::<Task>));
        on_cleanup({
            let pending = pending.clone();
            move || {
                if let Some(task) = pending.take() {
                    task.cancel();
                }
            }
        });
        let bubble = overlay(move || {
            let tip = move || {
                let caption = text(label.clone()).small().color(theme.elevated).no_wrap();
                let bubble = div()
                    .pass_through()
                    .px(8.0)
                    .py(4.0)
                    .rounded(6.0)
                    .bg(theme.text)
                    .child(caption);
                Placement::Below.place(bubble, anchor)
            };
            show(move || open.get(), tip)
                .enter(Pose::new().opacity(0.0), Tween::ms(120.0))
                .exit(Pose::new().opacity(0.0), Tween::ms(80.0))
        });
        self.root
            .on_hover(move |inside| {
                if let Some(task) = pending.take() {
                    task.cancel();
                }
                if inside {
                    pending.set(Some(spawn(async move {
                        sleep(delay).await;
                        open.set(true);
                    })));
                } else {
                    open.set(false);
                }
            })
            .child(bubble)
            .build(cx)
    }
}
