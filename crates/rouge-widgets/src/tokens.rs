use rouge_core::{Corners, Ease, Length, PropFrom, Spring, Timing, Tween};
use rouge_reactive::{Signal, provide_context, signal, use_context};

pub const UNIT: f32 = 4.0;
pub const HAIRLINE: f32 = 1.0;

pub const BASE_SIZE: f32 = 4.0 * UNIT;

pub const ROW_HEIGHT: f32 = 7.0 * UNIT;
pub const GROUP_HEIGHT: f32 = 8.0 * UNIT;
pub const BAR_HEIGHT: f32 = 10.0 * UNIT;
pub const ICON_BUTTON_SIZE: f32 = 6.0 * UNIT;
pub const ICON_SIZE: f32 = 4.0 * UNIT;
pub const ICON_SIZE_SMALL: f32 = 3.0 * UNIT;
pub const ICON_SIZE_LARGE: f32 = 5.0 * UNIT;
pub const CHECKBOX_SIZE: f32 = 4.0 * UNIT;
pub const CHECKBOX_SIZE_LARGE: f32 = 6.0 * UNIT;
pub const TREE_INDENT: f32 = Space::X3.px();
pub const SCROLLBAR_WIDTH: f32 = 3.0 * UNIT;
pub const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 8.0 * UNIT;

pub const GRID_COLUMNS: u16 = 12;
pub const GRID_GUTTER: Space = Space::X12;
pub const GRID_MARGIN: Space = Space::X6;

pub const DISABLED_OPACITY: f32 = 0.4;
pub const TINT_ALPHA: f32 = 0.16;
pub const PRESS_SCALE: f32 = 0.965;
pub const GRAB_SCALE: f32 = 1.2;
pub const POP_SCALE: f32 = 0.94;

/// Inset that keeps a [`Radius::Inner`] row concentric inside a [`Radius::Floating`] panel.
pub const FLOATING_INSET: f32 = Radius::Floating.px() - Radius::Inner.px();

pub const CONTROL_PADDING_X_SMALL: f32 = Space::X2.px();
pub const CONTROL_PADDING_X_MEDIUM: f32 = Space::X3.px();
pub const CONTROL_PADDING_X_LARGE: f32 = Space::X4.px();
pub const CONTROL_HEIGHT_LARGE: f32 = 11.0 * UNIT;

pub const MODAL_WIDTH_COMPACT: f32 = 440.0;
pub const MODAL_WIDTH: f32 = 560.0;
pub const MODAL_WIDTH_WIDE: f32 = 680.0;
pub const PICKER_WIDTH: f32 = 440.0;
pub const PICKER_LIST_HEIGHT: f32 = 360.0;
pub const PICKER_PREVIEW_LIST_WIDTH: f32 = POPOVER_WIDTH;
pub const PICKER_PREVIEW_WIDTH: f32 = POPOVER_WIDTH + PICKER_WIDTH;
pub const PICKER_PREVIEW_HEIGHT: f32 = PICKER_PREVIEW_WIDTH / 2.0;
pub const POPOVER_WIDTH: f32 = 520.0;
pub const MENU_WIDTH: f32 = 224.0;
pub const MODAL_TOP_OFFSET: f32 = Space::X24.px();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Space {
    X1 = 1,
    X2 = 2,
    X3 = 3,
    X4 = 4,
    X6 = 6,
    X8 = 8,
    X12 = 12,
    X16 = 16,
    X24 = 24,
}

impl Space {
    pub const ALL: [Space; 9] = [
        Space::X1,
        Space::X2,
        Space::X3,
        Space::X4,
        Space::X6,
        Space::X8,
        Space::X12,
        Space::X16,
        Space::X24,
    ];

    pub const fn units(self) -> f32 {
        self as u8 as f32
    }

    pub const fn px(self) -> f32 {
        self.units() * UNIT
    }
}

impl From<Space> for Length {
    fn from(space: Space) -> Self {
        Length::Px(space.px())
    }
}

impl PropFrom<Space> for Length {
    fn prop_from(space: Space) -> Self {
        space.into()
    }
}

/// Corner radius scale. Nested shapes stay concentric when inner radius = outer radius - padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radius {
    /// Rows, selections, segments and chips that sit inside a control or panel.
    Inner,
    /// Buttons, inputs, segmented controls and tooltips.
    Control,
    /// Panels that float above the page: menus, popovers, pickers, modals and the composer.
    Floating,
    /// Circles and pills for dots, tracks, thumbs and round icon buttons. Never use it on something holding text.
    Full,
}

impl Radius {
    pub const fn px(self) -> f32 {
        match self {
            Radius::Inner => 4.0,
            Radius::Control => 8.0,
            Radius::Floating => 12.0,
            // Renderers clamp a radius to half the shorter side; finite so scaling it on the GPU can't overflow.
            Radius::Full => 1.0e6,
        }
    }
}

impl From<Radius> for Corners {
    fn from(radius: Radius) -> Self {
        Corners::all(radius.px())
    }
}

impl PropFrom<Radius> for Corners {
    fn prop_from(radius: Radius) -> Self {
        radius.into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Press,
    Move,
    Enter,
    Exit,
    Layout,
    Gesture,
}

impl Pace {
    pub const fn spring(self) -> Spring {
        match self {
            Pace::Press | Pace::Move | Pace::Enter => Spring::new(600.0, 40.0),
            Pace::Exit => Spring::new(900.0, 60.0),
            Pace::Layout => Spring::new(260.0, 32.0),
            Pace::Gesture => Spring::new(320.0, 30.0),
        }
    }
}

impl From<Pace> for Timing {
    fn from(pace: Pace) -> Self {
        match MotionScale::current() {
            MotionScale::Full => pace.spring().into(),
            MotionScale::None => Tween::new(0.0).into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MotionScale {
    #[default]
    Full,
    None,
}

impl MotionScale {
    pub fn provide(self) -> Signal<MotionScale> {
        let scale = signal(self);
        provide_context(scale);
        scale
    }

    pub fn current() -> MotionScale {
        use_context::<Signal<MotionScale>>().map_or_default(|scale| scale.get_untracked())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hold(f32);

impl Hold {
    pub const fn seconds(seconds: f32) -> Self {
        Hold(seconds)
    }
}

impl From<Hold> for Timing {
    fn from(hold: Hold) -> Self {
        Tween::new(hold.0).ease(Ease::Linear).into()
    }
}
