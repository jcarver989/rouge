use crate::tokens::{RADIUS_CONTROL, RADIUS_FLOATING};
use rouge_core::{Apply, BoxShadow, Div, Glass, Mask, Point, Styled, Theme, View, div, px};

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
                    .bg(theme.surface)
                    .border(px(1.0))
                    .border_color(theme.border_subtle)
                    .rounded(RADIUS_CONTROL);
            }
            Material::Floating => {
                return self
                    .bg(theme.elevated)
                    .border(px(1.0))
                    .border_color(theme.border)
                    .rounded(RADIUS_FLOATING)
                    .shadow(BoxShadow::md());
            }
            Material::Glass => (theme.border, Some(RADIUS_FLOATING), Some(BoxShadow::lg())),
            Material::GlassPanel => (
                theme.border_subtle,
                Some(RADIUS_FLOATING),
                Some(BoxShadow::md()),
            ),
            Material::GlassControl => (theme.border_subtle, None, None),
        };
        let (blur, opacity) = if material == Material::Glass {
            (20.0, 0.86)
        } else {
            (12.0, 0.72)
        };
        self.border(px(1.0))
            .border_color(border)
            .backdrop(move || Glass::frosted(blur).tint(theme.elevated.get().with_alpha(opacity)))
            .when_some(radius, Styled::rounded)
            .when_some(shadow, Styled::shadow)
    }
}

pub fn scroll_edge(top: bool, solid: f32, reach: f32) -> impl View {
    let (from, to) = if top { (0.0, 1.0) } else { (1.0, 0.0) };
    let fade = Mask::linear_gradient(Point::new(0.0, from), Point::new(0.0, to))
        .stops(solid / (solid + reach), 1.0);
    let edge = div()
        .absolute()
        .left(0.0)
        .right(0.0)
        .h(solid + reach)
        .pass_through()
        .bg(Theme::current().canvas)
        .mask(fade);
    if top { edge.top(0.0) } else { edge.bottom(0.0) }
}
