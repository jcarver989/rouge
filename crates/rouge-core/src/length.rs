use rouge_render::Corners;
use std::cell::Cell;
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
    pub fn resolve(self, root: f32) -> f32 {
        match self {
            Length::Px(value) => value,
            Length::Rem(value) | Length::Fraction(value) => value * root,
            Length::Auto => 14.0,
        }
    }
}

thread_local! {
    static DESIGN_UNIT: Cell<Option<f32>> = const { Cell::new(None) };
}

pub fn set_design_unit(unit: Option<f32>) {
    DESIGN_UNIT.set(unit.filter(|unit| unit.is_finite() && *unit > 0.0));
}

pub fn design_unit() -> Option<f32> {
    DESIGN_UNIT.get()
}

impl Length {
    pub fn design(value: f32) -> Length {
        match design_unit() {
            Some(unit) => Length::Rem(value / unit),
            None => Length::Px(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Radii {
    pub corners: Corners,
    pub rem: bool,
}

impl Radii {
    pub fn all(length: Length) -> Radii {
        match length {
            Length::Px(value) => Radii {
                corners: Corners::all(value),
                rem: false,
            },
            Length::Rem(value) => Radii {
                corners: Corners::all(value),
                rem: true,
            },
            Length::Fraction(_) | Length::Auto => Radii::default(),
        }
    }

    pub(crate) fn resolve(self, root: f32) -> Corners {
        if self.rem {
            self.corners.map(|radius| radius * root)
        } else {
            self.corners
        }
    }
}
