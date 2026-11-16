use rouge::prelude::*;

pub(crate) use rouge::widgets::tokens::Radius;

pub(crate) const ACCENT: u32 = 0xf0d48c;

pub(crate) const NIGHT: Color = Color::rgb(0x03050c);
pub(crate) const STAR: Color = Color::rgb(ACCENT);
pub(crate) const ROSE: Color = Color::rgb(0xe8919a);
pub(crate) const FROST: Color = Color::rgb(0xeceff7);
pub(crate) const DUSK: Color = Color::rgb(0x95a0b8);

pub(crate) const HEADING: Color = FROST.with_alpha(0.9);
pub(crate) const FAINT: Color = DUSK.with_alpha(0.6);
pub(crate) const LINE: Color = FROST.with_alpha(0.07);
pub(crate) const WASH: Color = FROST.with_alpha(0.04);
pub(crate) const TINT: Color = FROST.with_alpha(0.09);
pub(crate) const DANGER_TINT: Color = ROSE.with_alpha(0.16);
pub(crate) const EDIT_RING: Color = STAR.with_alpha(0.6);

pub(crate) const UI_FONT: &str = "system-ui";
pub(crate) const TITLE_SIZE: f32 = 112.0;
pub(crate) const TITLE_WEIGHT: f32 = 100.0;
pub(crate) const TITLE_LINE_HEIGHT: f32 = 1.0;
pub(crate) const BODY_SIZE: f32 = 17.0;
pub(crate) const SMALL_SIZE: f32 = 13.0;

pub(crate) const GAP: f32 = 12.0;
pub(crate) const INSET: f32 = 16.0;
pub(crate) const STACK: f32 = 20.0;
pub(crate) const PAGE_TOP: f32 = 40.0;
pub(crate) const PAGE_BOTTOM: f32 = 48.0;
pub(crate) const HEADER_Y: f32 = 20.0;
pub(crate) const FOOTER_Y: f32 = 12.0;

pub(crate) const CARD_WIDTH: f32 = 560.0;
pub(crate) const COMPACT_WIDTH: f32 = 560.0;
pub(crate) const CARD_RADIUS: Corners = Corners::all(28.0);
pub(crate) const ROW_HEIGHT: f32 = 60.0;
pub(crate) const EDITOR_HEIGHT: f32 = 40.0;
pub(crate) const CONTROL: f32 = 32.0;
pub(crate) const CHECKBOX: f32 = 24.0;
pub(crate) const TAB_WIDTH: f32 = 92.0;
pub(crate) const TAB_HEIGHT: f32 = 28.0;

pub(crate) const HAIRLINE: f32 = 1.0;
pub(crate) const RING: f32 = 1.5;

pub(crate) const UNCHECKED_SCALE: f32 = 0.9;
pub(crate) const ROW_ENTER: Pose = Pose::new().opacity(0.0).translate_y(-10.0);
pub(crate) const ROW_EXIT: Pose = Pose::new().opacity(0.0).scale(0.97);

pub(crate) const GLASS: Glass = Glass::new()
    .blur(9.0)
    .tint(Color::new(0.04, 0.05, 0.11, 0.34))
    .refraction(1.0)
    .dispersion(0.3)
    .highlight(0.45)
    .edge_width(30.0)
    .shadow(0.4)
    .saturation(1.35)
    .rim_light(-0.7, 0.45);
