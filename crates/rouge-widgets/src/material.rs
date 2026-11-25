use crate::UiStyle;
use crate::tokens::Radius;
use rouge_core::{BoxShadow, Div, Glass, Role, Styled, Theme};

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Materials {
    #[default]
    Glass,
    Solid,
}

pub trait MaterialExt {
    #[must_use]
    fn floating_panel(self, emphasized: impl Fn() -> bool + 'static) -> Self;

    #[must_use]
    fn material(self, material: Material) -> Self;

    /// [`Material::Glass`] whose top edge brightens while `focused`.
    #[must_use]
    fn focusable_glass(self, focused: impl Fn() -> bool + 'static) -> Self;
}

impl MaterialExt for Div {
    fn material(self, material: Material) -> Self {
        UiStyle::current().materials.apply(self, material, || false)
    }

    fn floating_panel(self, emphasized: impl Fn() -> bool + 'static) -> Self {
        UiStyle::current()
            .materials
            .apply(self, Material::Glass, emphasized)
    }

    fn focusable_glass(self, focused: impl Fn() -> bool + 'static) -> Self {
        Materials::Glass.apply(self, Material::Glass, focused)
    }
}

impl Materials {
    fn apply(self, div: Div, material: Material, emphasized: impl Fn() -> bool + 'static) -> Div {
        let (radius, glass, tint) = match material {
            Material::Glass => (Radius::Floating, FROSTED, 0.5),
            Material::GlassControl => (Radius::Control, FROSTED, 0.5),
            Material::Lens => (Radius::Floating, LENS, 0.08),
            Material::Clear => (Radius::Floating, CLEAR, 0.0),
            Material::Surface => {
                return div
                    .bg(Role::Surface)
                    .border(1.0)
                    .border_color(Role::BorderSubtle)
                    .rounded(Radius::Control);
            }
            Material::Pigment => return div.bg(Role::Accent).text_color(Role::AccentForeground),
            Material::Bare => return div,
        };
        let div = div.rounded(radius);
        let floating = radius == Radius::Floating;
        match self {
            Self::Solid => {
                let div = div.bg(Role::Elevated).border(1.0);
                if floating {
                    div.shadow(BoxShadow::md()).border_color(move || {
                        if emphasized() {
                            Role::Accent
                        } else {
                            Role::Border
                        }
                    })
                } else {
                    div.border_color(Role::BorderSubtle)
                }
            }
            Self::Glass => {
                let theme = Theme::current();
                let glass = glass.shadow(if floating { 0.2 } else { 0.0 });
                div.backdrop(move || {
                    glass
                        .tint(theme.get().elevated.with_alpha(tint))
                        .highlight(if emphasized() { 0.28 } else { 0.17 })
                })
            }
        }
    }
}

const FROSTED: Glass = Glass::frosted(10.0).edge_width(1.5).rim_light(0.0, 0.0);
const LENS: Glass = Glass::new()
    .blur(6.0)
    .refraction(0.55)
    .dispersion(0.06)
    .rim_light(0.0, 0.0);
const CLEAR: Glass = Glass::new()
    .blur(0.0)
    .refraction(0.3)
    .dispersion(0.0)
    .rim_light(0.0, 0.0);
