use crate::fonts::{CODE_FONT, UI_FONT};
use rouge_core::{Theme, Typography, styles};

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

const fn preset(font: &'static str, size: f32, line_height: f32, weight: f32) -> Typography {
    Typography {
        font: Some(font),
        size,
        line_height: line_height / size,
        weight,
    }
}

styles! {
    pub trait UiStyles {
        caption(target) => target.typography(CAPTION);
        small(target) => target.typography(SMALL);
        label(target) => target.typography(LABEL);
        body(target) => target.typography(BODY);
        body_strong(target) => target.typography(BODY_STRONG);
        title(target) => target.typography(TITLE);
        heading(target) => target.typography(HEADING);
        display(target) => target.typography(DISPLAY);
        code(target) => target.typography(CODE);
        reading(target) => target.typography(READING);
        primary(target) => target.text_color(Theme::current().text);
        secondary(target) => target.text_color(Theme::current().text_secondary);
        muted(target) => target.text_color(Theme::current().text_muted);
        accent(target) => target.text_color(Theme::current().accent);
    }
}
