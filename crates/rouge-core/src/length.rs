use taffy::{Dimension, LengthPercentage, LengthPercentageAuto};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Length {
    Px(f32),
    Fraction(f32),
    #[default]
    Auto,
}

pub fn px(value: f32) -> Length {
    Length::Px(value)
}

pub fn pct(value: f32) -> Length {
    Length::Fraction(value / 100.0)
}

pub fn auto() -> Length {
    Length::Auto
}

impl From<Length> for Dimension {
    fn from(value: Length) -> Self {
        LengthPercentageAuto::from(value).into()
    }
}

impl From<Length> for LengthPercentageAuto {
    fn from(value: Length) -> Self {
        match value {
            Length::Px(v) => LengthPercentageAuto::length(v),
            Length::Fraction(v) => LengthPercentageAuto::percent(v),
            Length::Auto => LengthPercentageAuto::auto(),
        }
    }
}

impl From<Length> for LengthPercentage {
    fn from(value: Length) -> Self {
        match value {
            Length::Px(v) => LengthPercentage::length(v),
            Length::Fraction(v) => LengthPercentage::percent(v),
            Length::Auto => LengthPercentage::length(0.0),
        }
    }
}
