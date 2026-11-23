pub mod access;
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
mod presentation;
mod prop;
mod scroll;
mod selection;
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
    Commit, CompositorActor, CompositorEvent, Content, EffectSlot, Events, LayerChanges,
    LayerDelta, LayerId, LayerModel, LayerPose, Layers, Mailbox, Message, MotionId, Nested,
    ScrollLinked, ScrollState, SurfaceOp, WindowConfig, WindowId, mailbox,
};
#[cfg(not(target_arch = "wasm32"))]
pub use compositor::{ThreadError, spawn};
pub use control::{Each, IntoItems, Show, dynamic, each, overlay, show, switch};
pub use editable::{Editable, editable};
pub use event::{
    APPLE, Chord, ChordError, Cursor, DragAxis, DragEvent, DragPhase, FileDropEvent, FileDropPhase,
    FocusEvent, ImeEvent, IntoChord, Key, KeyEvent, Modifiers, PointerButton, PointerEvent,
    ScrollEvent, TouchPhase,
};
pub use focus::{FocusHandle, focus_handle};
pub use image::{Fit, Image, ImageLoadError, ImageSource, decode_image, image};
pub use keymap::use_binding;
pub use length::{Length, auto, pct, px};
pub use motion::{Animation, Completion, MappedMotion, Motion, MotionProp, motion};
pub use path::{Path, PathBuilder};
pub use presentation::{COMPACT_WIDTH, InputKind, Presentation, WidthClass, presentation};
pub use prop::{AnyEffect, IntoProp, Prop, PropFrom};
pub use rouge_theme::{Rgba, Role, UiPalette};
pub use scroll::{
    Align, RowHeight, RowKey, Scrollable, VirtualList, VirtualListHandle, VirtualRows, scroll,
    virtual_list, virtual_list_handle, virtual_row_index,
};
pub use selection::TextBreak;
pub use style::{
    BoxShadow, Declaration, DelegateStyle, Slot, StyleDelta, StyleState, Styled, Typography,
};
pub use surface::{Surface, surface};
pub use theme::{Fill, Palette, Theme};
pub use tree::{FrameStats, Interaction, PaintRegion};
pub use ui::{Ui, Wake, keyboard_inset, modifiers, safe_area, window_scope, window_size, zoom};
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
