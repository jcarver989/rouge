use crate::style::UiStyle;
use rouge_core::PropFrom;
use std::borrow::Cow;

pub const UI_FONT: &str = "Inter";
pub const CODE_FONT: &str = "Lilex";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontRole {
    Ui,
    Code,
}

impl FontRole {
    pub fn name(self) -> Cow<'static, str> {
        let fonts = UiStyle::current().fonts;
        match self {
            Self::Ui => fonts.ui,
            Self::Code => fonts.code,
        }
    }
}

impl PropFrom<FontRole> for Cow<'static, str> {
    fn prop_from(role: FontRole) -> Self {
        role.name()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fonts {
    pub ui: Cow<'static, str>,
    pub code: Cow<'static, str>,
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            ui: UI_FONT.into(),
            code: CODE_FONT.into(),
        }
    }
}

const FACES: [&[u8]; 8] = [
    include_bytes!("../assets/fonts/inter/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/inter/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/inter/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/inter/Inter-Bold.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-Regular.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-Bold.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-Italic.ttf"),
    include_bytes!("../assets/fonts/lilex/Lilex-BoldItalic.ttf"),
];

pub fn bundled_fonts() -> impl Iterator<Item = &'static [u8]> {
    FACES.into_iter()
}
