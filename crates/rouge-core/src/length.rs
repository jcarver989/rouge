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

macro_rules! into_taffy {
    ($($ty:ident => $auto:expr;)*) => {$(
        impl From<Length> for $ty {
            fn from(value: Length) -> Self {
                match value {
                    Length::Px(v) => $ty::length(v),
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
