use crate::prop::PropFrom;
use rouge_reactive::{Memo, ReadSignal, Signal, expect_context, provide_context, signal};
use rouge_render::Color;
use rouge_theme::{
    ButtonVariant, ControlState, NoticeTone, Role, SelectionState, SemanticStyle, UiPalette,
};
use std::ops::Deref;

pub type Palette = UiPalette<Color>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    Color(Color),
    Role(Role),
}

impl Fill {
    pub fn resolve(self, palette: &Palette) -> Color {
        match self {
            Fill::Color(color) => color,
            Fill::Role(role) => *palette.get(role),
        }
    }

    pub(crate) fn is_visible(self) -> bool {
        !matches!(self, Fill::Color(color) if color.a <= 0.0)
    }
}

impl Default for Fill {
    fn default() -> Self {
        Fill::Color(Color::TRANSPARENT)
    }
}

impl PropFrom<Color> for Fill {
    fn prop_from(color: Color) -> Self {
        Fill::Color(color)
    }
}

impl PropFrom<u32> for Fill {
    fn prop_from(hex: u32) -> Self {
        Fill::Color(Color::rgb(hex))
    }
}

impl PropFrom<Role> for Fill {
    fn prop_from(role: Role) -> Self {
        Fill::Role(role)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme(Signal<Palette>);

impl Theme {
    pub fn new<T: Into<Color>>(palette: UiPalette<T>) -> Theme {
        Theme(signal(palette.map(Into::into)))
    }

    pub fn provide<T: Into<Color>>(palette: UiPalette<T>) -> Theme {
        let theme = Theme::new(palette);
        provide_context(theme);
        theme
    }

    #[track_caller]
    pub fn current() -> Theme {
        expect_context()
    }

    pub fn set<T: Into<Color>>(&self, palette: UiPalette<T>) {
        self.0.set_if_changed(palette.map(Into::into));
    }

    #[track_caller]
    pub fn color(self, role: Role) -> Memo<Color> {
        self.0.select(move |palette| *palette.get(role))
    }

    pub fn resolve(&self, fill: Fill) -> Color {
        self.0.with(|palette| fill.resolve(palette))
    }

    pub fn control_style(variant: ButtonVariant, state: ControlState) -> SemanticStyle<Fill> {
        UiPalette::ROLES
            .control_style(variant, state)
            .map(Fill::Role)
    }

    pub fn selection_style(state: SelectionState) -> SemanticStyle<Fill> {
        UiPalette::ROLES.selection_style(state).map(Fill::Role)
    }

    pub fn notice_style(tone: NoticeTone) -> SemanticStyle<Fill> {
        UiPalette::ROLES.notice_style(tone).map(Fill::Role)
    }
}

impl Deref for Theme {
    type Target = ReadSignal<Palette>;

    fn deref(&self) -> &ReadSignal<Palette> {
        &self.0
    }
}
