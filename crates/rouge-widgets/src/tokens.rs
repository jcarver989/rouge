use rouge_core::{Corners, PropFrom};

pub const BASE_SIZE: f32 = 16.0;

pub const ROW_HEIGHT: f32 = 28.0;
pub const GROUP_HEIGHT: f32 = 32.0;
pub const BAR_HEIGHT: f32 = 40.0;
pub const ICON_BUTTON_SIZE: f32 = 24.0;
pub const ICON_SIZE: f32 = 16.0;
pub const ICON_SIZE_SMALL: f32 = 12.0;
pub const CHECKBOX_SIZE: f32 = 12.0;
pub const TREE_INDENT: f32 = 12.0;
pub const SCROLLBAR_WIDTH: f32 = 12.0;
pub const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 32.0;

pub const DISABLED_OPACITY: f32 = 0.4;

/// Inset that keeps a [`Radius::Inner`] row concentric inside a [`Radius::Floating`] panel.
pub const FLOATING_INSET: f32 = Radius::Floating.px() - Radius::Inner.px();

pub const CONTROL_PADDING_X_SMALL: f32 = 8.0;
pub const CONTROL_PADDING_X_MEDIUM: f32 = 12.0;

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
pub const MODAL_TOP_OFFSET: f32 = 96.0;

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
