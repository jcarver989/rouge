mod canvas;
mod clipboard;
mod control;
mod editable;
mod event;
mod focus;
mod image;
mod keymap;
mod length;
mod motion;
mod path;
mod prop;
mod scroll;
mod style;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
mod theme;
mod tree;
mod ui;
mod view;
mod window;

pub use canvas::{Canvas, Icon, Painter, TextHit, TextRun, canvas, icon};
pub use clipboard::{Clipboard, ClipboardTarget, MemoryClipboard};
pub use control::{Each, Show, dynamic, each, overlay, show};
pub use editable::{Editable, editable};
pub use event::{
    Chord, ChordError, Cursor, DragEvent, DragPhase, FocusEvent, ImeEvent, Key, KeyEvent,
    Modifiers, PointerButton, PointerEvent, ScrollEvent,
};
pub use focus::{FocusHandle, focus_handle};
pub use image::{Fit, Image, ImageLoadError, ImageSource, decode_image, image};
pub use keymap::use_binding;
pub use length::{Length, Radii, auto, design_unit, pct, px, rem, set_design_unit};
pub use motion::{Animation, Motion, MotionProp, motion};
pub use path::{Path, PathBuilder};
pub use prop::{IntoEffect, IntoProp, Prop, PropFrom};
pub use scroll::{
    Align, RowHeight, RowKey, Scrollable, VirtualList, VirtualListHandle, scroll, virtual_list,
    virtual_list_handle,
};
pub use style::{BoxShadow, Dirty, GroupStyles, Style, Styled, Typography};
pub use theme::{Palette, Theme, provide_theme, use_theme};
pub use tree::{ElementId, FrameStats, PaintRegion};
pub use ui::{Frame, Ui, root_font_size, window_size};
pub use view::{
    AnyView, Apply, BuildCx, Div, FocusKind, Interactions, Text, View, div, rich_text, text,
};

#[doc(hidden)]
pub mod __private {
    pub use rouge_reactive::{Signal, batch, expect_context, provide_context, signal};
}

#[doc(hidden)]
pub use rouge_macros::__element;
pub use rouge_macros::{component, view};

pub use rouge_render::{
    Affine, BackdropBlur, BlendMode, Blur, Color, ColorMatrix, Corners, Decay, Ease, Edges, Effect,
    Glass, ImageData, ImageError, Inputs, Keyframes, LayoutTransition, Mask, Point, Pose, Quad,
    Rect, ScrollLinked, ScrollState, Size, Spring, Timing, Tween,
};
pub use rouge_text::{SpanStyle, TextAlign, TextStats, TextStyle, Underline};

pub use window::{
    Appearance, Decorations, EditAction, Menu, MenuItem, ResizeEdge, WindowCommand, WindowHandle,
    appearance, window_handle,
};
