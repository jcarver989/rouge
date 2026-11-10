use super::patch::{Status, Tone};
use super::syntax::Token;
use rouge::prelude::*;

pub(crate) use super::super::palette::{
    ACCENT, BASE, FAINT, GREEN, MAUVE, MUTED, PEACH, RED, TEXT, YELLOW,
};

pub(crate) const SELECTION: Color = ACCENT.with_alpha(0.28);
pub(crate) const HUNK: Color = ACCENT.with_alpha(0.1);

pub(crate) const UI_FONT: &str = "system-ui";
pub(crate) const CODE_FONT: &str = "Menlo, DejaVu Sans Mono, monospace";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ToneColors {
    pub background: Color,
    pub hover: Color,
    pub indicator: Color,
    pub gutter: Color,
}

pub(crate) const fn tone(tone: Tone) -> ToneColors {
    let tint = match tone {
        Tone::Added => GREEN,
        Tone::Removed => RED,
        Tone::Context | Tone::Meta | Tone::HunkHeader => {
            return ToneColors {
                background: Color::TRANSPARENT,
                hover: Color::WHITE.with_alpha(0.05),
                indicator: Color::TRANSPARENT,
                gutter: FAINT,
            };
        }
    };
    ToneColors {
        background: tint.with_alpha(0.13),
        hover: tint.with_alpha(0.2),
        indicator: tint,
        gutter: tint.with_alpha(0.75),
    }
}

pub(crate) const fn token(token: Token) -> Color {
    match token {
        Token::Plain => TEXT,
        Token::Keyword => MAUVE,
        Token::Type => YELLOW,
        Token::Function => ACCENT,
        Token::String => GREEN,
        Token::Number => PEACH,
        Token::Comment => FAINT,
    }
}

pub(crate) const fn status(status: Status) -> Color {
    match status {
        Status::Added | Status::Untracked => GREEN,
        Status::Deleted => RED,
        Status::Modified => YELLOW,
        Status::Renamed | Status::Copied => ACCENT,
    }
}
