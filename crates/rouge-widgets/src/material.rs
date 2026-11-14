use crate::tokens::{RADIUS_CONTROL, RADIUS_FLOATING};
use rouge_core::{Apply, BoxShadow, Div, Glass, Styled, Theme};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Material {
    #[default]
    Floating,
    Surface,
    Glass,
    GlassPanel,
    GlassControl,
    Bare,
}

pub trait MaterialExt {
    #[must_use]
    fn material(self, material: Material) -> Self;
}

impl MaterialExt for Div {
    fn material(self, material: Material) -> Self {
        let theme = Theme::current();
        let (border, radius, shadow) = match material {
            Material::Bare => return self,
            Material::Surface => {
                return self
                    .bg(theme.surface())
                    .border(1.0)
                    .border_color(theme.border_subtle())
                    .rounded(RADIUS_CONTROL);
            }
            Material::Floating => {
                return self
                    .bg(theme.elevated())
                    .border(1.0)
                    .border_color(theme.border())
                    .rounded(RADIUS_FLOATING)
                    .shadow(BoxShadow::md());
            }
            Material::Glass => (theme.border(), Some(RADIUS_FLOATING), Some(BoxShadow::lg())),
            Material::GlassPanel => (
                theme.border_subtle(),
                Some(RADIUS_FLOATING),
                Some(BoxShadow::md()),
            ),
            Material::GlassControl => (theme.border_subtle(), None, None),
        };
        let (blur, opacity) = if material == Material::Glass {
            (20.0, 0.86)
        } else {
            (12.0, 0.72)
        };
        self.border(1.0)
            .border_color(border)
            .backdrop(move || Glass::frosted(blur).tint(theme.get().elevated.with_alpha(opacity)))
            .when_some(radius, Styled::rounded)
            .when_some(shadow, Styled::shadow)
    }
}
