use rouge_core::{
    AnyView, BoxShadow, BuildCx, Div, ElementId, Key, PointerEvent, Pose, Rect, Styled, Theme,
    Tween, View, div, overlay, show, styled, text, use_theme, window_size,
};
use rouge_reactive::{Signal, Task, signal, sleep, spawn};
use std::cell::Cell;
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
        let (panel, window) = (panel.absolute(), window_size());
        let left = move || anchor.get().origin.x;
        let top = move || anchor.get().origin.y;
        match self {
            Placement::Below => panel.left(left).top(move || anchor.get().max().y + GAP),
            Placement::Above => panel
                .left(left)
                .bottom(move || window.get().height - top() + GAP),
            Placement::Right => panel.left(move || anchor.get().max().x + GAP).top(top),
            Placement::Left => panel
                .right(move || window.get().width - left() + GAP)
                .top(top),
        }
    }
}

pub(crate) fn panel(theme: Theme) -> Div {
    div()
        .bg(theme.surface)
        .border(1.0)
        .border_color(theme.border)
        .rounded(theme.radius)
        .shadow(BoxShadow::md())
}

pub(crate) fn popup(
    open: Signal<bool>,
    dismissable: bool,
    backdrop: impl Fn(Div) -> Div + 'static,
    content: impl Fn() -> Div + 'static,
) -> impl View {
    let dismiss = move || {
        if dismissable {
            open.set(false);
        }
    };
    overlay(move || {
        let layer = move || {
            let panel = content()
                .on_pointer_down(PointerEvent::stop_propagation)
                .on_key_down(move |event| {
                    if event.key == Key::Escape {
                        dismiss();
                        event.stop_propagation();
                    }
                });
            let layer = div()
                .absolute()
                .inset(0.0)
                .on_pointer_down(move |_| dismiss());
            backdrop(layer).child(panel)
        };
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
}

pub fn popover<T: View, U: View>(
    open: Signal<bool>,
    trigger: T,
    content: impl Fn() -> U + 'static,
) -> Popover {
    let (root, anchor) = anchored(trigger);
    Popover {
        root,
        open,
        anchor,
        content: Box::new(move || content().into_any()),
        placement: Placement::default(),
    }
}

impl Popover {
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }
}

styled!(Popover => root);

impl View for Popover {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let theme = use_theme();
        let (anchor, placement, content) = (self.anchor, self.placement, self.content);
        let body = move || {
            let body = panel(theme).focus_scope().autofocus().child(content());
            placement.place(body, anchor)
        };
        self.root
            .child(popup(self.open, true, |layer| layer, body))
            .build(cx)
    }
}

pub struct Tooltip {
    root: Div,
    label: String,
    delay: Duration,
    anchor: Signal<Rect>,
}

pub fn tooltip(target: impl View, label: impl Into<String>) -> Tooltip {
    let (root, anchor) = anchored(target);
    Tooltip {
        root,
        label: label.into(),
        delay: Duration::from_millis(500),
        anchor,
    }
}

impl Tooltip {
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

styled!(Tooltip => root);

impl View for Tooltip {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let theme = use_theme();
        let (label, delay, anchor) = (self.label, self.delay, self.anchor);
        let open = signal(false);
        let pending = Cell::new(None::<Task>);
        let bubble = overlay(move || {
            let tip = move || {
                let caption = text(label.clone())
                    .color(theme.surface)
                    .text_size(12.0)
                    .no_wrap();
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
