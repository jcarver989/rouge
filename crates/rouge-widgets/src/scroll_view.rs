use rouge_core::{
    BuildCx, Div, DragPhase, ElementId, Length, Point, PointerButton, PointerEvent, ScrollLinked,
    ScrollState, Scrollable, Styled, View, div, px, root_font_size, show, styled, use_theme,
};
use rouge_reactive::{Signal, memo, signal};
use std::cell::Cell;

const MIN_THUMB: f32 = 24.0;

#[derive(Clone, Copy)]
pub struct ScrollbarStyle {
    pub track: Length,
    pub inset: Length,
    pub thumb: Length,
    pub min_thumb: Length,
    pub radius: Length,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self {
            track: Length::design(10.0),
            inset: Length::design(2.0),
            thumb: Length::design(6.0),
            min_thumb: Length::design(MIN_THUMB),
            radius: Length::design(3.0),
        }
    }
}

pub fn vertical_scrollbar(state: Signal<ScrollState>, style: ScrollbarStyle) -> impl View {
    scrollbar_if_needed(state, Axis::Vertical, style)
}

fn scrollbar_if_needed(state: Signal<ScrollState>, axis: Axis, style: ScrollbarStyle) -> impl View {
    show(
        move || axis.content(state.get()) > axis.viewport(state.get()) + 0.5,
        move || scrollbar(state, axis, style),
    )
}

pub struct ScrollView<T> {
    root: Div,
    content: T,
    state: Option<Signal<ScrollState>>,
}

pub fn scroll_view<T: Scrollable + Styled>(content: T) -> ScrollView<T> {
    ScrollView {
        root: div().relative(),
        content,
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
        let bars = [Axis::Vertical, Axis::Horizontal]
            .map(|axis| scrollbar_if_needed(state, axis, ScrollbarStyle::default()));
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

fn scrollbar(state: Signal<ScrollState>, axis: Axis, style: ScrollbarStyle) -> Div {
    let theme = use_theme();
    let root = root_font_size();
    let length = memo(move || {
        let state = state.get();
        let viewport = axis.viewport(state);
        (viewport * viewport / axis.content(state).max(1.0))
            .clamp(viewport.min(style.min_thumb.resolve(root.get())), viewport)
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
        .rounded(style.radius)
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
                .w(style.track),
            thumb
                .test_id("scroll-thumb-vertical")
                .top(0.0)
                .right(style.inset)
                .w(style.thumb)
                .h(move || px(length.get()))
                .scroll_linked(state, link),
        ),
        Axis::Horizontal => (
            div()
                .test_id("scroll-track-horizontal")
                .left(0.0)
                .right(0.0)
                .bottom(0.0)
                .h(style.track),
            thumb
                .test_id("scroll-thumb-horizontal")
                .left(0.0)
                .bottom(style.inset)
                .h(style.thumb)
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
