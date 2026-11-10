use rouge_core::{
    BuildCx, Div, ElementId, ScrollState, Scrollable, Styled, View, div, show, styled, use_theme,
};
use rouge_reactive::{Signal, memo, signal};

const MIN_THUMB: f32 = 24.0;

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
        let theme = use_theme();
        let state = self.state.unwrap_or_else(|| signal(ScrollState::default()));
        let scrollable =
            move || state.with(|state| state.content.height > state.viewport.height + 0.5);
        let length = memo(move || {
            let ScrollState {
                viewport, content, ..
            } = state.get();
            (viewport.height * viewport.height / content.height.max(1.0))
                .clamp(MIN_THUMB, viewport.height.max(MIN_THUMB))
        });
        let travel = move || {
            let state = state.get();
            let range = (state.content.height - state.viewport.height).max(1.0);
            (state.offset.y / range).clamp(0.0, 1.0)
                * (state.viewport.height - length.get()).max(0.0)
        };
        let thumb = move || {
            div()
                .absolute()
                .top(0.0)
                .right(2.0)
                .w(6.0)
                .rounded(3.0)
                .pass_through()
                .bg(move || theme.text_muted.get().with_alpha(0.5))
                .h(length)
                .translate_y(travel)
        };
        self.root
            .child(self.content.size_full().scroll_state(state))
            .child(show(scrollable, thumb))
            .build(cx)
    }
}
