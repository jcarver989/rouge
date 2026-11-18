use crate::icon::{Icon, IconName};
use crate::material::{Material, MaterialExt};
use crate::popover::POPPED;
use crate::progress::spinning;
use crate::tokens::{
    GROUP_HEIGHT, ICON_SIZE, Pace, Radius, SCROLLBAR_MIN_THUMB_HEIGHT, SCROLLBAR_WIDTH,
};
use rouge_core::component;
use rouge_core::view;
use rouge_core::{
    BuildCx, DelegateStyle, Div, DragPhase, Point, PointerButton, PointerEvent, Role, ScrollLinked,
    ScrollState, Scrollable, Styled, Theme, View, div, dynamic, show,
};
use rouge_reactive::{Signal, effect, memo, signal};
use std::cell::Cell;

const THUMB_INSET: f32 = 2.0;
const THUMB_WIDTH: f32 = 8.0;

#[component]
pub fn vertical_scrollbar(state: Signal<ScrollState>) -> impl View {
    scrollbar_if_needed(state, Axis::Vertical)
}

fn scrollbar_if_needed(state: Signal<ScrollState>, axis: Axis) -> impl View {
    show(
        move || axis.content(state.get()) > axis.viewport(state.get()) + 0.5,
        move || scrollbar(state, axis),
    )
}

pub struct ScrollView<T> {
    root: Div,
    content: T,
    state: Option<Signal<ScrollState>>,
    refreshing: Option<Signal<bool>>,
}

#[component]
pub fn scroll_view<T: Scrollable + Styled>(children: T) -> ScrollView<T> {
    ScrollView {
        root: div().relative(),
        content: children,
        state: None,
        refreshing: None,
    }
}

impl<T> ScrollView<T> {
    pub fn refresh(mut self, refreshing: Signal<bool>) -> Self {
        self.refreshing = Some(refreshing);
        self
    }
}

impl<T: Scrollable + Styled> Scrollable for ScrollView<T> {
    fn scroll_state(mut self, state: Signal<ScrollState>) -> Self {
        self.state = Some(state);
        self
    }
}

impl<T> DelegateStyle for ScrollView<T> {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.root
    }
}

impl<T: Scrollable + Styled> View for ScrollView<T> {
    fn build(self, cx: &mut BuildCx<'_>) {
        let state = self.state.unwrap_or_else(|| signal(ScrollState::default()));
        let bars = [Axis::Vertical, Axis::Horizontal].map(|axis| scrollbar_if_needed(state, axis));
        let refresher = self
            .refreshing
            .map(|refreshing| refresher(state, refreshing));
        self.root
            .child(self.content.size_full().scroll_state(state))
            .children(bars)
            .child(refresher)
            .build(cx);
    }
}

fn refresher(state: Signal<ScrollState>, refreshing: Signal<bool>) -> impl View {
    let pull = memo(move || (-state.get().offset.y).max(0.0));
    effect(move || {
        if pull.get() >= REFRESH_PULL && !refreshing.get_untracked() {
            refreshing.set(true);
        }
    });
    let badge = move || {
        let mark = view! {
            <dynamic>
                {if refreshing.get() {
                    spinning(ICON_SIZE, Role::Accent)
                        .test_id("refresh-spinner")
                        .into_any()
                } else {
                    Icon::new(IconName::ArrowDown, Role::TextMuted).into_any()
                }}
            </dynamic>
        };
        let disc = view! {
            <div flex items_center justify_center size=GROUP_HEIGHT material=Material::Glass
                 rounded=Radius::Full>
                {mark}
            </div>
        };
        view! {
            <div test_id="refresh-indicator" absolute top=8.0 left=0.0 right=0.0 flex justify_center
                 pass_through
                 opacity={move || {
                     if refreshing.get() {
                         1.0
                     } else {
                         (pull.get() / REFRESH_PULL).min(1.0)
                     }
                 }}>
                {disc}
            </div>
        }
    };
    view! {
        <show when={move || refreshing.get() || pull.get() > 0.0} children=badge
             enter=(POPPED, Pace::Enter) exit=(POPPED, Pace::Exit)/>
    }
}

const REFRESH_PULL: f32 = 48.0;

#[derive(Clone, Copy)]
enum Axis {
    Vertical,
    Horizontal,
}

impl Axis {
    fn point(self, point: Point) -> f32 {
        match self {
            Self::Vertical => point.y,
            Self::Horizontal => point.x,
        }
    }

    fn viewport(self, state: ScrollState) -> f32 {
        match self {
            Self::Vertical => state.viewport.height,
            Self::Horizontal => state.viewport.width,
        }
    }

    fn content(self, state: ScrollState) -> f32 {
        match self {
            Self::Vertical => state.content.height,
            Self::Horizontal => state.content.width,
        }
    }

    fn scroll_to(self, state: Signal<ScrollState>, offset: f32) {
        state.update(|state| {
            let mut point = state.offset;
            match self {
                Self::Vertical => point.y = offset,
                Self::Horizontal => point.x = offset,
            }
            state.scroll_to(point);
        });
    }
}

fn scrollbar(state: Signal<ScrollState>, axis: Axis) -> Div {
    let theme = Theme::current();
    let length = memo(move || {
        let state = state.get();
        let viewport = axis.viewport(state);
        (viewport * viewport / axis.content(state).max(1.0))
            .clamp(viewport.min(SCROLLBAR_MIN_THUMB_HEIGHT), viewport)
            .round()
    });
    let extent = memo(move || {
        let state = state.get();
        (axis.viewport(state), axis.content(state))
    });
    let link = move || {
        let (viewport, content) = extent.get();
        let travel = (viewport - length.get()).max(0.0);
        ScrollLinked::Linear {
            factor: travel / (content - viewport).max(1.0),
            max: travel,
            horizontal: matches!(axis, Axis::Horizontal),
        }
    };
    let origin = Cell::new(0.0);
    let thumb = div()
        .absolute()
        .rounded(Radius::Full)
        .bg(move || theme.get().text_muted.with_alpha(0.5))
        .on_pointer_down(PointerEvent::stop_propagation)
        .on_drag(move |event| {
            let current = state.get_untracked();
            if event.phase == DragPhase::Start {
                origin.set(axis.point(current.offset));
            }
            let range = (axis.content(current) - axis.viewport(current)).max(0.0);
            let travel = (axis.viewport(current) - length.get_untracked()).max(1.0);
            axis.scroll_to(
                state,
                origin.get() + axis.point(event.translation) * range / travel,
            );
        });
    let (track, thumb) = match axis {
        Axis::Vertical => (
            div()
                .test_id("scroll-track-vertical")
                .top(0.0)
                .bottom(0.0)
                .right(0.0)
                .w(SCROLLBAR_WIDTH),
            thumb
                .test_id("scroll-thumb-vertical")
                .top(0.0)
                .right(THUMB_INSET)
                .w(THUMB_WIDTH)
                .h(move || length.get())
                .scroll_linked(state, link),
        ),
        Axis::Horizontal => (
            div()
                .test_id("scroll-track-horizontal")
                .left(0.0)
                .right(0.0)
                .bottom(0.0)
                .h(SCROLLBAR_WIDTH),
            thumb
                .test_id("scroll-thumb-horizontal")
                .left(0.0)
                .bottom(THUMB_INSET)
                .h(THUMB_WIDTH)
                .w(move || length.get())
                .scroll_linked(state, link),
        ),
    };
    track
        .absolute()
        .preserve_focus()
        .on_pointer_down(move |event| {
            if event.button == PointerButton::Primary {
                let current = state.get_untracked();
                let range = (axis.content(current) - axis.viewport(current)).max(0.0);
                let travel = (axis.viewport(current) - length.get_untracked()).max(1.0);
                axis.scroll_to(
                    state,
                    (axis.point(event.local()) - length.get_untracked() / 2.0) * range / travel,
                );
                event.stop_propagation();
            }
        })
        .child(thumb)
}
