use crate::tokens::{SCROLLBAR_MIN_THUMB_HEIGHT, SCROLLBAR_WIDTH};
use rouge_core::component;
use rouge_core::{
    BuildCx, Div, DragPhase, ElementId, Length, Point, PointerButton, PointerEvent, ScrollLinked,
    ScrollState, Scrollable, Styled, Theme, View, div, px, root_font_size, show, styled,
};
use rouge_reactive::{Signal, memo, signal};
use std::cell::Cell;

const THUMB_INSET: f32 = 2.0;
const THUMB_WIDTH: f32 = 8.0;
const THUMB_RADIUS: f32 = 4.0;

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
}

#[component]
pub fn scroll_view<T: Scrollable + Styled>(children: T) -> ScrollView<T> {
    ScrollView {
        root: div().relative(),
        content: children,
        state: None,
    }
}

impl<T: Scrollable + Styled> Scrollable for ScrollView<T> {
    fn scroll_state(mut self, state: Signal<ScrollState>) -> Self {
        self.state = Some(state);
        self
    }
}

styled!(ScrollView<T> => root);

impl<T: Scrollable + Styled> View for ScrollView<T> {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let state = self.state.unwrap_or_else(|| signal(ScrollState::default()));
        let bars = [Axis::Vertical, Axis::Horizontal].map(|axis| scrollbar_if_needed(state, axis));
        self.root
            .child(self.content.size_full().scroll_state(state))
            .children(bars)
            .build(cx)
    }
}

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
    let root = root_font_size();
    let length = memo(move || {
        let state = state.get();
        let viewport = axis.viewport(state);
        (viewport * viewport / axis.content(state).max(1.0))
            .clamp(
                viewport.min(Length::design(SCROLLBAR_MIN_THUMB_HEIGHT).resolve(root.get())),
                viewport,
            )
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
        .rounded(THUMB_RADIUS)
        .bg(move || theme.text_muted.get().with_alpha(0.5))
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
                .h(move || px(length.get()))
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
                .w(move || px(length.get()))
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
