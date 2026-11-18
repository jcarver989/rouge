use rouge::prelude::*;
use rouge::widgets::tokens::{POP_SCALE, Space, UNIT};

pub(crate) const TITLE_SIZE: f32 = 72.0;
pub(crate) const TITLE_LINE_HEIGHT: f32 = 1.0;

pub(crate) const CARD_WIDTH: f32 = 140.0 * UNIT;
pub(crate) const EDITOR_HEIGHT: f32 = 8.0 * UNIT;
pub(crate) const TABS_HEIGHT: f32 = 8.0 * UNIT;

pub(crate) const READING_FADE: f32 = Space::X24.px();

pub(crate) const RING_LIMIT: usize = 12;
pub(crate) const RING_RADIUS: f32 = 170.0 * UNIT;
pub(crate) const RING_PITCH: f32 = Space::X8.px();
pub(crate) const RING_ECHO: f32 = Space::X2.px();
pub(crate) const RING_INK: f32 = 0.75 * UNIT;
pub(crate) const RING_GRAPHITE: f32 = 0.5;
pub(crate) const RING_ORIGIN: Point = Point::new(42.0 * UNIT, 15.0 * UNIT);
pub(crate) const RING_ENTER: Pose = Pose::new().opacity(0.0).scale(1.06);
pub(crate) const RING_EXIT: Pose = Pose::new().opacity(0.0).scale(1.06);

pub(crate) const WAVE_SECONDS: f32 = 2.0;
pub(crate) const WAVE_SPEED: f32 = 130.0 * UNIT;
pub(crate) const WAVE_AMPLITUDE: f32 = 15.0;
pub(crate) const WAVE_DAMPING: f32 = 1.1;
pub(crate) const WAVE_LENGTH: f32 = 21.0 * UNIT;

pub(crate) const ROW_ENTER: Pose = Pose::new().opacity(0.0).translate_y(-Space::X2.px());
pub(crate) const ROW_EXIT: Pose = Pose::new().opacity(0.0).scale(POP_SCALE);
