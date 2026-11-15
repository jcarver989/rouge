mod animation;
mod canvas;
mod clipboard;
mod compositor;
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
pub mod text;
mod theme;
mod tree;
mod ui;
mod view;
mod window;

pub use animation::{
    Decay, Ease, Keyframes, LayerProp, LayoutTransition, Pose, Spring, Timing, Tween,
};
pub use canvas::{Canvas, Continuation, Icon, Measure, Painter, TextHit, TextRun, canvas, icon};
pub use clipboard::{Clipboard, ClipboardMedia, MemoryClipboard, PasteRequest};
pub use compositor::{
    Compositor, Content, EffectSlot, LayerId, LayerPose, MotionId, Nested, ScrollLinked,
    ScrollState,
};
pub use control::{Each, IntoItems, Show, dynamic, each, overlay, show, switch};
pub use editable::{Editable, editable};
pub use event::{
    Chord, ChordError, Cursor, DragEvent, DragPhase, FileDropEvent, FileDropPhase, FocusEvent,
    ImeEvent, IntoChord, Key, KeyEvent, Modifiers, PointerButton, PointerEvent, ScrollEvent,
    TouchPhase,
};
pub use focus::{FocusHandle, focus_handle};
pub use image::{Fit, Image, ImageLoadError, ImageSource, decode_image, image};
pub use keymap::use_binding;
pub use length::{Length, auto, pct, px};
pub use motion::{Animation, Completion, Motion, MotionProp, motion};
pub use path::{Path, PathBuilder};
pub use prop::{AnyEffect, IntoProp, Prop, PropFrom};
pub use rouge_theme::{Rgba, Role, UiPalette};
pub use scroll::{
    Align, RowHeight, RowKey, Scrollable, VirtualList, VirtualListHandle, scroll, virtual_list,
    virtual_list_handle,
};
pub use style::{BoxShadow, Declaration, Slot, StyleDelta, Styled, Typography};
pub use surface::{Surface, surface};
pub use theme::{Fill, Palette, Theme};
pub use tree::{FrameStats, PaintRegion};
pub use ui::{Frame, Ui, Wake, modifiers, window_size, zoom};
pub use view::{AnyView, Apply, BuildCx, Div, FocusPolicy, Text, View, div, rich_text, text};

#[doc(hidden)]
pub use rouge_macros::__element;
pub use rouge_macros::{component, view};

pub use rouge_render::{
    Affine, BackdropBlur, BlendMode, Blur, Color, ColorMatrix, Corners, Edges, Glass, ImageData,
    ImageError, Inputs, Mask, Point, Quad, Rect, ShaderEffect, Size, SurfaceError, SurfaceProducer,
    SurfaceSource, SurfaceStatus,
};
pub use text::{SpanStyle, TextAlign, TextStats, TextStyle, Underline, WrapIndent};

pub use window::{
    Appearance, Decorations, EditAction, Menu, MenuAction, MenuItem, ResizeEdge, WindowCommand,
    WindowHandle, appearance, route, window_focused, window_handle,
};
