use taffy::{Dimension, LengthPercentage, LengthPercentageAuto};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Length {
    Px(f32),
    Rem(f32),
    Fraction(f32),
    #[default]
    Auto,
}

pub fn px(value: f32) -> Length {
    Length::Px(value)
}

pub fn rem(value: f32) -> Length {
    Length::Rem(value)
}

pub fn pct(value: f32) -> Length {
    Length::Fraction(value / 100.0)
}

pub fn auto() -> Length {
    Length::Auto
}

macro_rules! into_taffy {
    ($($ty:ident => $auto:expr;)*) => {$(
        impl From<Length> for $ty {
            fn from(value: Length) -> Self {
                match value {
                    Length::Px(v) => $ty::length(v),
                    Length::Rem(v) => $ty::length(v * 16.0),
                    Length::Fraction(v) => $ty::percent(v),
                    Length::Auto => $auto,
                }
            }
        }
    )*};
}

into_taffy! {
    Dimension => Dimension::auto();
    LengthPercentageAuto => LengthPercentageAuto::auto();
    LengthPercentage => LengthPercentage::length(0.0);
}

impl Length {
    pub(crate) fn resolve(self, root: f32) -> f32 {
        match self {
            Length::Px(value) => value,
            Length::Rem(value) | Length::Fraction(value) => value * root,
            Length::Auto => 14.0,
        }
    }
}
