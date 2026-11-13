mod app;
#[cfg(feature = "tokio-rt")]
pub mod persist;
pub mod platform;
mod profile;

pub use app::{App, AppError, WindowOptions};
pub use rouge_core as core;
pub use rouge_core::{
    Appearance, Decorations, EditAction, Menu, MenuItem, ResizeEdge, WindowHandle, appearance,
    window_focused, window_handle,
};
#[cfg(feature = "markdown")]
pub use rouge_markdown as markdown;
pub use rouge_reactive as reactive;
pub use rouge_render as render;
#[cfg(feature = "syntax")]
pub use rouge_syntax as syntax;
pub use rouge_widgets as widgets;

pub mod prelude {
    pub use crate::app::{App, WindowOptions};
    pub use rouge_core::{
        Align, AnyView, Appearance, Apply, BackdropBlur, BlendMode, Blur, BoxShadow, Chord,
        ClipboardMedia, Color, ColorMatrix, Corners, Cursor, Decay, Decorations, Div, DragEvent,
        DragPhase, Ease, Edges, EditAction, Fit, FocusEvent, FocusHandle, Glass, ImageData,
        ImageSource, ImeEvent, Inputs, Interactions, Key, KeyEvent, Keyframes, LayoutTransition,
        Length, Mask, Menu, MenuItem, Modifiers, Motion, Painter, Palette, Path, Point,
        PointerButton, PointerEvent, Pose, Rect, ResizeEdge, Rgba, RowKey, ScrollEvent,
        ScrollState, Scrollable, Size, SpanStyle, Spring, Style, Styled, Text, TextAlign,
        TextStyle, Theme, Tween, Typography, UiPalette, Underline, View, VirtualListHandle,
        WindowHandle, appearance, auto, canvas, component, div, dynamic, each, editable,
        focus_handle, icon, image, motion, overlay, pct, px, rem, rich_text, root_font_size,
        scroll, set_design_unit, show, switch, text, use_binding, view, virtual_list,
        virtual_list_handle, window_focused, window_handle, window_size,
    };
    #[cfg(not(target_arch = "wasm32"))]
    pub use rouge_reactive::spawn_blocking;
    pub use rouge_reactive::{
        Callback, Effect, Emitter, Memo, ReadSignal, Resource, Runtime, Scope, Signal, Stored,
        Task, TaskError, batch, effect, emitter, expect_context, interval, memo, on_cleanup,
        provide_context, resource, signal, sleep, spawn, stored, untrack, use_context, watch,
    };
    #[cfg(feature = "tokio-rt")]
    pub use rouge_reactive::{set_tokio, spawn_tokio};
    pub use rouge_widgets::{
        ButtonVariant, CheckState, Hint, Material, MaterialExt, MenuFocus, ModalSize, Placement,
        RovingFocus, Side, button, checkbox, context_menu, menu_bar, modal, popover, progress,
        radio, resizable_panel, scroll_view, select, separator, slider, spinner, tabs, text_area,
        text_input, toggle, tooltip, window_chrome,
    };
}
