mod app;
pub mod platform;
mod profile;

pub use app::{App, AppError, WindowOptions};
pub use rouge_core as core;
pub use rouge_core::{
    Appearance, Decorations, EditAction, Menu, MenuItem, ResizeEdge, WindowHandle, appearance,
    window_focused, window_handle,
};
pub use rouge_reactive as reactive;
pub use rouge_render as render;
pub use rouge_widgets as widgets;

pub mod prelude {
    pub use crate::app::{App, WindowOptions};
    pub use rouge_core::{
        Align, AnyView, Appearance, Apply, BackdropBlur, BlendMode, Blur, BoxShadow, Chord,
        ClipboardMedia, Color, ColorMatrix, Corners, Cursor, Decay, Decorations, Div, DragEvent,
        DragPhase, Ease, Edges, EditAction, Fit, FocusEvent, FocusHandle, Glass, ImageData,
        ImageSource, ImeEvent, Inputs, Interactions, Key, KeyEvent, Keyframes, LayoutTransition,
        Length, Mask, Menu, MenuItem, Modifiers, Motion, Painter, Palette, Path, Point,
        PointerButton, PointerEvent, Pose, Rect, ResizeEdge, RowKey, ScrollEvent, ScrollState,
        Scrollable, Size, SpanStyle, Spring, Style, Styled, Text, TextAlign, TextStyle, Theme,
        Tween, Typography, Underline, View, VirtualListHandle, WindowHandle, appearance, auto,
        canvas, component, div, dynamic, each, editable, focus_handle, icon, image, motion,
        overlay, pct, provide_theme, px, rem, rich_text, root_font_size, scroll, set_design_unit,
        show, switch, text, use_binding, use_theme, view, virtual_list, virtual_list_handle,
        window_focused, window_handle, window_size,
    };
    pub use rouge_reactive::{
        Callback, Effect, Emitter, Memo, ReadSignal, Resource, Runtime, Scope, Signal, Stored,
        Task, TaskError, background, batch, effect, emitter, expect_context, interval, memo,
        on_cleanup, provide_context, resource, run_in, signal, sleep, spawn, stored, untrack,
        use_context, watch,
    };
    #[cfg(feature = "tokio-rt")]
    pub use rouge_reactive::{set_tokio, spawn_blocking, spawn_tokio};
    pub use rouge_widgets::{
        Hint, MenuFocus, Placement, RovingFocus, Side, button, checkbox, context_menu, menu_bar,
        menu_item, modal, popover, progress, radio, resizable_panel, scroll_view, select,
        separator, slider, spinner, tabs, text_area, text_input, toggle, tooltip, window_chrome,
    };
}
