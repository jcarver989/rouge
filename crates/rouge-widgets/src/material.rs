use crate::tokens::Radius;
use rouge_core::{Div, Glass, Role, Styled, Theme};

/// The surface a panel is drawn with. Every layer that floats above the page is [`Material::Glass`],
/// so menus, tooltips, pickers and the composer read as one material.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Material {
    /// Frosted glass lit from overhead for panels floating above the page.
    #[default]
    Glass,
    /// [`Material::Glass`] at control size for controls resting on the page.
    GlassControl,
    Lens,
    Clear,
    /// An opaque card in the page's flow.
    Surface,
    Pigment,
    Bare,
}

pub trait MaterialExt {
    #[must_use]
    fn material(self, material: Material) -> Self;

    /// [`Material::Glass`] whose top edge brightens while `focused`.
    #[must_use]
    fn focusable_glass(self, focused: impl Fn() -> bool + 'static) -> Self;
}

impl MaterialExt for Div {
    fn material(self, material: Material) -> Self {
        match material {
            Material::Glass => self.focusable_glass(|| false),
            Material::GlassControl => {
                overhead_glass(self.rounded(Radius::Control), FROSTED, 0.0, || false)
            }
            Material::Lens | Material::Clear => {
                let recipe = if material == Material::Lens {
                    LENS
                } else {
                    CLEAR
                };
                overhead_glass(
                    self.rounded(Radius::Floating),
                    recipe,
                    FLOATING_SHADOW,
                    || false,
                )
            }
            Material::Surface => self
                .bg(Role::Surface)
                .border(1.0)
                .border_color(Role::BorderSubtle)
                .rounded(Radius::Control),
            Material::Pigment => self.bg(Role::Accent).text_color(Role::AccentForeground),
            Material::Bare => self,
        }
    }

    fn focusable_glass(self, focused: impl Fn() -> bool + 'static) -> Self {
        overhead_glass(
            self.rounded(Radius::Floating),
            FROSTED,
            FLOATING_SHADOW,
            focused,
        )
    }
}

/// Flat frosted glass lit from overhead: the top edge catches the light, the sides fade and the bottom
/// stays dark.
fn overhead_glass(
    div: Div,
    recipe: Recipe,
    shadow: f32,
    focused: impl Fn() -> bool + 'static,
) -> Div {
    let theme = Theme::current();
    div.backdrop(move || {
        let highlight = if focused() {
            FOCUSED_HIGHLIGHT
        } else {
            HIGHLIGHT
        };
        recipe
            .glass
            .shadow(shadow)
            .tint(theme.get().elevated.with_alpha(recipe.tint))
            .highlight(highlight)
    })
}

#[derive(Clone, Copy)]
struct Recipe {
    glass: Glass,
    tint: f32,
}

const FROSTED: Recipe = Recipe {
    glass: Glass::frosted(10.0).edge_width(1.5).rim_light(0.0, 0.0),
    tint: 0.5,
};

const LENS: Recipe = Recipe {
    glass: Glass::new()
        .blur(6.0)
        .refraction(0.55)
        .dispersion(0.06)
        .rim_light(0.0, 0.0),
    tint: 0.08,
};

const CLEAR: Recipe = Recipe {
    glass: Glass::new()
        .blur(0.0)
        .refraction(0.3)
        .dispersion(0.0)
        .rim_light(0.0, 0.0),
    tint: 0.0,
};

const FLOATING_SHADOW: f32 = 0.2;

const HIGHLIGHT: f32 = 0.17;

const FOCUSED_HIGHLIGHT: f32 = 0.28;
