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
    /// An opaque card in the page's flow.
    Surface,
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
            Material::GlassControl => overhead_glass(self.rounded(Radius::Control), || false),
            Material::Surface => self
                .bg(Role::Surface)
                .border(1.0)
                .border_color(Role::BorderSubtle)
                .rounded(Radius::Control),
            Material::Bare => self,
        }
    }

    fn focusable_glass(self, focused: impl Fn() -> bool + 'static) -> Self {
        overhead_glass(
            self.rounded(Radius::Floating).shadow(BoxShadow::md()),
            focused,
        )
    }
}

/// Flat frosted glass lit from overhead: the top edge catches the light, the sides fade and the bottom
/// stays dark.
fn overhead_glass(div: Div, focused: impl Fn() -> bool + 'static) -> Div {
    let theme = Theme::current();
    div.backdrop(move || {
        let highlight = if focused() {
            FOCUSED_HIGHLIGHT
        } else {
            HIGHLIGHT
        };
        OVERHEAD_GLASS
            .tint(theme.get().elevated.with_alpha(TINT))
            .highlight(highlight)
    })
}

const OVERHEAD_GLASS: Glass = Glass::frosted(10.0)
    .edge_width(1.5)
    .shadow(0.2)
    .rim_light(0.0, 0.0);

const TINT: f32 = 0.5;

const HIGHLIGHT: f32 = 0.17;

const FOCUSED_HIGHLIGHT: f32 = 0.28;
