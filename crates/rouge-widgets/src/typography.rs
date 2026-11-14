use crate::fonts::{CODE_FONT, UI_FONT};
use rouge_core::{Styled, Theme, Typography};

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

pub trait UiStyles: Styled {
    fn caption(self) -> Self {
        self.typography(CAPTION)
    }

    fn small(self) -> Self {
        self.typography(SMALL)
    }

    fn label(self) -> Self {
        self.typography(LABEL)
    }

    fn body(self) -> Self {
        self.typography(BODY)
    }

    fn body_strong(self) -> Self {
        self.typography(BODY_STRONG)
    }

    fn title(self) -> Self {
        self.typography(TITLE)
    }

    fn heading(self) -> Self {
        self.typography(HEADING)
    }

    fn display(self) -> Self {
        self.typography(DISPLAY)
    }

    fn code(self) -> Self {
        self.typography(CODE)
    }

    fn reading(self) -> Self {
        self.typography(READING)
    }

    #[track_caller]
    fn primary(self) -> Self {
        self.text_color(Theme::current().text())
    }

    #[track_caller]
    fn secondary(self) -> Self {
        self.text_color(Theme::current().text_secondary())
    }

    #[track_caller]
    fn muted(self) -> Self {
        self.text_color(Theme::current().text_muted())
    }

    #[track_caller]
    fn accent(self) -> Self {
        self.text_color(Theme::current().accent())
    }
}

impl<T: Styled> UiStyles for T {}
