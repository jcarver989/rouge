use crate::components::ControlSize;
use crate::icon::IconName;
use crate::tokens::{
    CONTROL_HEIGHT_LARGE, GROUP_HEIGHT, HAIRLINE, Hold, PRESS_SCALE, Pace, Radius,
};
use crate::typography::UiStyles;
use rouge_core::view;
use rouge_core::{
    Color, Cursor, Div, Editable, Fill, FocusHandle, LayoutTransition, Prop, Rect, Role, Styled,
    Theme, View, div, dynamic, editable, motion, presentation,
};
use rouge_reactive::{Memo, Signal, effect, memo, signal, spawn, use_context};
use rouge_theme::NoticeTone;
use std::cell::Cell;
use std::fmt::Display;
use std::rc::Rc;

pub(crate) fn fade_when_disabled(root: Div, disabled: &Rc<Prop<bool>>) -> Div {
    let disabled = disabled.clone();
    root.opacity(move || {
        if disabled.get() {
            crate::tokens::DISABLED_OPACITY
        } else {
            1.0
        }
    })
}

pub(crate) fn row_height() -> impl Fn() -> f32 + Copy + 'static {
    let presentation = presentation();
    move || ControlSize::of(presentation.get()).metrics().0
}

pub(crate) fn is_touch() -> bool {
    presentation().get_untracked().is_touch()
}

pub(crate) fn pressable(root: Div) -> Div {
    let pressed = signal(false);
    root.on_press(move |down| pressed.set(down))
        .scale(move || if pressed.get() { PRESS_SCALE } else { 1.0 })
        .transition(Pace::Press)
}

pub(crate) fn by_touch<T>(touch: T, pointer: T) -> T {
    if is_touch() { touch } else { pointer }
}

pub(crate) fn field_height() -> f32 {
    by_touch(CONTROL_HEIGHT_LARGE, GROUP_HEIGHT)
}

pub(crate) fn hit_box(glyph: Div) -> Div {
    if is_touch() {
        view! {
            <div flex flex_none items_center justify_center min_w=CONTROL_HEIGHT_LARGE min_h=CONTROL_HEIGHT_LARGE>
                {glyph}
            </div>
        }
    } else {
        glyph
    }
}

pub(crate) fn long_press(root: Div, action: impl Fn() + 'static) -> Div {
    let (clock, action) = (motion(0.0), Rc::new(action));
    root.on_press(move |down| {
        clock.set(0.0);
        if down {
            let (held, action) = (clock.animate_to(1.0, LONG_PRESS), action.clone());
            spawn(async move {
                if held.await {
                    action();
                }
            });
        }
    })
}

const LONG_PRESS: Hold = Hold::seconds(0.5);

pub(crate) fn tone_icon(tone: NoticeTone) -> IconName {
    match tone {
        NoticeTone::Error | NoticeTone::Warning => IconName::TriangleAlert,
        NoticeTone::Positive => IconName::CircleCheck,
        _ => IconName::Info,
    }
}

pub(crate) trait TestId {
    fn part(&self, name: impl Display) -> Option<String>;
}

impl TestId for Option<String> {
    fn part(&self, name: impl Display) -> Option<String> {
        self.as_ref().map(|id| format!("{id}-{name}"))
    }
}

pub(crate) fn entry(value: Signal<String>, focus: FocusHandle) -> Editable {
    view! {
        <editable value=value w_full body color=Role::Text
             caret_color=Theme::current().get_untracked().accent track_focus=focus/>
    }
}

pub(crate) fn entry_frame(root: Div, focus: FocusHandle) -> Div {
    root.border(HAIRLINE)
        .rounded(Radius::Control)
        .bg(Role::Elevated)
        .cursor(Cursor::Text)
        .border_color(control_edge(move || focus.is_focused()))
        .on_pointer_down(move |_| focus.focus())
}

pub(crate) fn focus_ring(root: Div) -> Div {
    root.focus_visible(|style| style.border_color(Role::Accent))
}

pub(crate) fn focus_frame(root: Div) -> Div {
    let shown = signal(false);
    let edge = move || {
        if shown.get() {
            Fill::Role(Role::Accent)
        } else {
            Fill::Color(Color::TRANSPARENT)
        }
    };
    let ring = view! {
        <div absolute inset=0.0 pass_through rounded=Radius::Control border=HAIRLINE border_color=edge/>
    };
    root.on_focus(move |event| shown.set(event.visible))
        .on_blur(move |_| shown.set(false))
        .child(ring)
}

pub(crate) fn follower(
    container: Signal<Rect>,
    target: impl Fn() -> Option<Rect> + 'static,
    decorate: impl FnOnce(Div) -> Div + 'static,
) -> impl View {
    let target = memo(move || target().filter(|at| at.size.width > 0.0));
    let placed = signal(None::<Rect>);
    effect(move || {
        if let Some(at) = target.get() {
            let origin = container.get().origin;
            let at = Rect::new(
                at.origin.x - origin.x,
                at.origin.y - origin.y,
                at.size.width,
                at.size.height,
            );
            placed.set_if_changed(Some(at));
        }
    });
    let ready = memo(move || placed.with(Option::is_some));
    let pending = Cell::new(Some(decorate));
    view! {
        <dynamic>
            {
                let decorate = ready.get().then(|| pending.take()).flatten()?;
                let geometry = move |f: fn(Rect) -> f32| move || placed.get().map_or(0.0, f);
                let layer = view! {
                    <div absolute left=geometry(|at| at.origin.x) top=geometry(|at| at.origin.y)
                         w=geometry(|at| at.size.width) h=geometry(|at| at.size.height)
                         layout_transition=LayoutTransition::new(Pace::Move).size()
                         when=(move || target.with(Option::is_none), Styled::invisible)/>
                };
                Some(decorate(layer))
            }
        </dynamic>
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Spot {
    held: Signal<Option<(u64, Rect)>>,
    container: Signal<Rect>,
}

impl Spot {
    pub(crate) fn new() -> Self {
        Spot {
            held: signal(None),
            container: signal(Rect::ZERO),
        }
    }

    pub(crate) fn contain(self, container: Div) -> Div {
        container.track_bounds(self.container)
    }

    pub(crate) fn track(self, item: Div, on: impl Fn() -> bool + 'static) -> Div {
        let bounds = signal(Rect::ZERO);
        let (held, key) = (self.held, bounds.id());
        effect(move || {
            let at = bounds.get();
            if on() && at.size.width > 0.0 {
                held.set_if_changed(Some((key, at)));
            } else if held.with_untracked(|held| held.is_some_and(|(held, _)| held == key)) {
                held.set(None);
            }
        });
        item.track_bounds(bounds)
    }

    pub(crate) fn follower(self, decorate: impl FnOnce(Div) -> Div + 'static) -> impl View {
        let held = self.held;
        follower(
            self.container,
            move || held.get().map(|(_, at)| at),
            decorate,
        )
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Invalid(pub(crate) Memo<bool>);

pub(crate) fn control_edge(focused: impl Fn() -> bool + 'static) -> impl Fn() -> Color {
    let (theme, invalid) = (Theme::current(), use_context::<Invalid>());
    move || {
        let palette = theme.get();
        if invalid.is_some_and(|invalid| invalid.0.get()) {
            palette.destructive
        } else if focused() {
            palette.accent
        } else {
            palette.border
        }
    }
}

macro_rules! chrome {
    (impl[$($generics:tt)*] $widget:ty) => {
        impl<$($generics)*> $widget {
            pub fn test_id(mut self, id: impl Into<String>) -> Self {
                self.test_id = Some(id.into());
                self
            }
        }

        impl<$($generics)*> DelegateStyle for $widget {
            fn style_target(&mut self) -> &mut (impl Styled + Default) {
                &mut self.style
            }
        }
    };
    ($widget:ty) => {
        chrome!(impl[] $widget);
    };
}
pub(crate) use chrome;
