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
mod surface;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
mod theme;
mod tree;
mod ui;
mod view;
mod window;

pub use canvas::{Canvas, Continuation, Icon, Measure, Painter, TextHit, TextRun, canvas, icon};
pub use clipboard::{Clipboard, ClipboardMedia, ClipboardTarget, MemoryClipboard};
pub use control::{Each, Show, dynamic, each, overlay, show, switch};
pub use editable::{Editable, editable};
pub use event::{
    Chord, ChordError, Cursor, DragEvent, DragPhase, FileDropEvent, FileDropPhase, FocusEvent,
    ImeEvent, IntoChord, Key, KeyEvent, Modifiers, PointerButton, PointerEvent, ScrollEvent,
};
pub use focus::{FocusHandle, focus_handle};
pub use image::{Fit, Image, ImageLoadError, ImageSource, decode_image, image};
pub use keymap::use_binding;
pub use length::{Length, Radii, auto, design_unit, pct, px, rem, set_design_unit};
pub use motion::{Animation, Motion, MotionProp, motion};
pub use path::{Path, PathBuilder};
pub use prop::{IntoEffect, IntoProp, Prop, PropFrom};
pub use rouge_theme::{Rgba, UiPalette};
pub use scroll::{
    Align, RowHeight, RowKey, Scrollable, VirtualList, VirtualListHandle, scroll, virtual_list,
    virtual_list_handle,
};
pub use style::{BoxShadow, Dirty, GroupStyles, Style, Styled, Typography};
pub use surface::{Surface, surface};
pub use theme::{Palette, Theme};
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
    Rect, ScrollLinked, ScrollState, Size, Spring, SurfaceError, SurfaceProducer, SurfaceSource,
    SurfaceStatus, Timing, Tween,
};
pub use rouge_text::{SpanStyle, TextAlign, TextStats, TextStyle, Underline, WrapIndent};

pub use window::{
    Appearance, Decorations, EditAction, Menu, MenuAction, MenuItem, ResizeEdge, WindowCommand,
    WindowHandle, appearance, window_focused, window_handle,
};
