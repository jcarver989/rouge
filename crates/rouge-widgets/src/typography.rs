use crate::fonts::{CODE_FONT, FontRole, UI_FONT};
use rouge_core::{Role, Styled, Typography};

pub const CAPTION: Typography = preset(UI_FONT, 11.0, 16.0, 400.0);
pub const SMALL: Typography = preset(UI_FONT, 12.0, 16.0, 400.0);
pub const LABEL: Typography = preset(UI_FONT, 12.0, 16.0, 500.0);
pub const BODY: Typography = preset(UI_FONT, 13.0, 20.0, 400.0);
pub const BODY_STRONG: Typography = preset(UI_FONT, 13.0, 20.0, 500.0);
pub const TITLE: Typography = preset(UI_FONT, 14.0, 20.0, 500.0);
pub const HEADING: Typography = preset(UI_FONT, 16.0, 24.0, 600.0);
pub const DISPLAY: Typography = preset(UI_FONT, 20.0, 28.0, 600.0);
pub const CODE: Typography = preset(CODE_FONT, 13.0, 20.0, 400.0);
pub const READING: Typography = preset(UI_FONT, 15.0, 24.0, 400.0);

pub(crate) const fn preset(
    font: &'static str,
    size: f32,
    line_height: f32,
    weight: f32,
) -> Typography {
    Typography {
        font: Some(font),
        size,
        line_height: line_height / size,
        weight,
    }
}

pub(crate) fn apply_preset<T: Styled>(target: T, preset: Typography, role: FontRole) -> T {
    target
        .typography(Typography {
            font: None,
            ..preset
        })
        .font(role)
}

pub trait UiStyles: Styled {
    fn caption(self) -> Self {
        apply_preset(self, CAPTION, FontRole::Ui)
    }

    fn small(self) -> Self {
        apply_preset(self, SMALL, FontRole::Ui)
    }

    fn label(self) -> Self {
        apply_preset(self, LABEL, FontRole::Ui)
    }

    fn body(self) -> Self {
        apply_preset(self, BODY, FontRole::Ui)
    }

    fn body_strong(self) -> Self {
        apply_preset(self, BODY_STRONG, FontRole::Ui)
    }

    fn title(self) -> Self {
        apply_preset(self, TITLE, FontRole::Ui)
    }

    fn heading(self) -> Self {
        apply_preset(self, HEADING, FontRole::Ui)
    }

    fn display(self) -> Self {
        apply_preset(self, DISPLAY, FontRole::Ui)
    }

    fn code(self) -> Self {
        apply_preset(self, CODE, FontRole::Code)
    }

    fn reading(self) -> Self {
        apply_preset(self, READING, FontRole::Ui)
    }

    #[track_caller]
    fn primary(self) -> Self {
        self.text_color(Role::Text)
    }

    #[track_caller]
    fn secondary(self) -> Self {
        self.text_color(Role::TextSecondary)
    }

    #[track_caller]
    fn muted(self) -> Self {
        self.text_color(Role::TextMuted)
    }

    #[track_caller]
    fn accent(self) -> Self {
        self.text_color(Role::Accent)
    }
}

impl<T: Styled> UiStyles for T {}
