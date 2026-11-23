use crate::kbd::kbd;
use crate::material::{Material, MaterialExt};
use crate::sheet::Presenter;
use crate::tokens::{POP_SCALE, Pace, Radius, Space};
use crate::typography::UiStyles;
use rouge_core::Role;
use rouge_core::component;
use rouge_core::view;
use rouge_core::{
    AnyView, BuildCx, Chord, DelegateStyle, Div, IntoChord, Key, PointerEvent, Pose, Rect, Styled,
    View, div, dynamic, overlay, presentation, show, text, window_size,
};
use rouge_reactive::{Signal, Task, memo, on_cleanup, signal, sleep, spawn};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

const GAP: f32 = 4.0;

pub(crate) const POPPED: Pose = Pose::new().opacity(0.0).scale(POP_SCALE);
pub(crate) const FADED: Pose = Pose::new().opacity(0.0);

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
            .max_w(move || window.get().width)
            .max_h(move || window.get().height)
            .track_bounds(measured)
            .left(0.0)
            .top(0.0)
            .translate_x(move || position().0)
            .translate_y(move || position().1)
    }
}

pub(crate) fn dismiss_layer(layer: Div, dismiss: impl Fn() + Clone + 'static) -> Div {
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
}

pub(crate) fn dismissable(layer: Div, panel: Div, dismiss: impl Fn() + Clone + 'static) -> Div {
    dismiss_layer(layer, dismiss).child(panel.on_pointer_down(PointerEvent::stop_propagation))
}

pub(crate) fn floating(
    open: impl Fn() -> bool + 'static,
    hidden: Pose,
    panel: impl Fn() -> Div + 'static,
) -> impl View {
    overlay(move || {
        view! {
            <show when=open children=panel enter=(hidden, Pace::Enter) exit=(hidden, Pace::Exit)/>
        }
    })
}

pub(crate) fn popup(
    open: Signal<bool>,
    backdrop: impl Fn(Div) -> Div + 'static,
    place: impl Fn(Div) -> Div + 'static,
    content: impl Fn() -> Div + 'static,
) -> impl View {
    let presentation = presentation();
    let handheld = memo(move || presentation.get().is_handheld());
    let (backdrop, place, content) = (Rc::new(backdrop), Rc::new(place), Rc::new(content));
    view! {
        <dynamic>
            {
                let (backdrop, place, content) = (backdrop.clone(), place.clone(), content.clone());
                if handheld.get() {
                    Presenter::new(open)
                        .present(move || content().into_any())
                        .into_any()
                } else {
                    floating(
                        move || open.get(),
                        POPPED,
                        move || dismissable(backdrop(view! { <div/> }), place(content()), move || open.set(false)),
                    )
                    .into_any()
                }
            }
        </dynamic>
    }
}

fn anchored(target: impl View) -> (Div, Signal<Rect>) {
    let anchor = signal(Rect::ZERO);
    (div().track_bounds(anchor).child(target), anchor)
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
        root: root.on_activate(move || open.toggle()),
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

impl DelegateStyle for Popover {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.root
    }
}

impl View for Popover {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (anchor, placement, content, material) =
            (self.anchor, self.placement, self.content, self.material);
        let body = move || view! { <div focus_scope autofocus>{content()}</div> };
        let place = move |body: Div| placement.place(body.material(material), anchor);
        self.root
            .child(popup(self.open, |layer| layer, place, body))
            .build(cx);
    }
}

pub struct Tooltip {
    root: Div,
    label: String,
    shortcut: Option<Chord>,
    content: Option<Rc<dyn Fn() -> AnyView>>,
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
            shortcut: None,
            content: None,
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
    #[track_caller]
    pub fn shortcut(mut self, chord: impl IntoChord) -> Self {
        self.shortcut = Some(chord.into_chord());
        self
    }

    pub fn content<T: View>(mut self, content: impl Fn() -> T + 'static) -> Self {
        self.content = Some(Rc::new(move || content().into_any()));
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

impl DelegateStyle for Tooltip {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.root
    }
}

impl View for Tooltip {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (label, delay, anchor) = (self.label, self.delay, self.anchor);
        let (shortcut, content) = (self.shortcut, self.content);
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
        let bubble = floating(
            move || open.get(),
            POPPED,
            move || {
                let body = match &content {
                    Some(content) => content(),
                    None => view! { <text small color=Role::Text no_wrap>{label.clone()}</text> }
                        .into_any(),
                };
                let keys = shortcut
                    .as_ref()
                    .map(|chord| view! { <kbd>{chord.display()}</kbd> });
                let bubble = view! {
                    <div pass_through flex items_center gap=Space::X2 px=Space::X2 py=Space::X1
                         material=Material::Glass rounded=Radius::Control>
                        {body}
                        {keys}
                    </div>
                };
                Placement::Below.place(bubble, anchor)
            },
        );
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
            .build(cx);
    }
}

pub struct HoverCard {
    open: Box<dyn Fn() -> bool>,
    anchor: Signal<Rect>,
    content: Rc<dyn Fn() -> AnyView>,
    placement: Placement,
    material: Material,
}

#[component]
pub fn hover_card<T: View>(
    open: impl Fn() -> bool + 'static,
    anchor: Signal<Rect>,
    children: impl Fn() -> T + 'static,
) -> HoverCard {
    HoverCard {
        open: Box::new(open),
        anchor,
        content: Rc::new(move || children().into_any()),
        placement: Placement::default(),
        material: Material::default(),
    }
}

impl HoverCard {
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    pub fn material(mut self, material: Material) -> Self {
        self.material = material;
        self
    }
}

impl View for HoverCard {
    fn build(self, cx: &mut BuildCx<'_>) {
        let HoverCard {
            open,
            anchor,
            content,
            placement,
            material,
        } = self;
        floating(open, POPPED, move || {
            let panel = div()
                .material(material)
                .on_pointer_down(PointerEvent::stop_propagation)
                .child(content());
            placement.place(panel, anchor)
        })
        .build(cx);
    }
}
