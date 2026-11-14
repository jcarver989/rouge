use rouge_reactive::{Memo, ReadSignal, Signal, expect_context, provide_context, signal};
use rouge_render::Color;
use rouge_theme::{
    ButtonVariant, ControlState, NoticeTone, SelectionState, SemanticStyle, UiPalette,
};
use std::ops::Deref;

pub type Palette = UiPalette<Color>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme(Signal<Palette>);

macro_rules! roles {
    ($($role:ident),* $(,)?) => {$(
        #[track_caller]
        pub fn $role(self) -> Memo<Color> {
            self.0.select(|palette| palette.$role)
        }
    )*};
}

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

    pub fn control_style(
        &self,
        variant: ButtonVariant,
        state: ControlState,
    ) -> SemanticStyle<Color> {
        self.get().control_style(variant, state)
    }

    pub fn selection_style(&self, state: SelectionState) -> SemanticStyle<Color> {
        self.get().selection_style(state)
    }

    pub fn notice_style(&self, tone: NoticeTone) -> SemanticStyle<Color> {
        self.get().notice_style(tone)
    }

    roles!(
        canvas,
        surface,
        surface_hover,
        surface_selected,
        chrome,
        elevated,
        input,
        text,
        text_secondary,
        text_muted,
        border,
        border_subtle,
        accent,
        accent_foreground,
        info,
        positive,
        warning,
        destructive,
        destructive_foreground,
        scrim,
    );
}

impl Deref for Theme {
    type Target = ReadSignal<Palette>;

    fn deref(&self) -> &ReadSignal<Palette> {
        &self.0
    }
}
