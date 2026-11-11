mod app;
pub mod platform;

pub use app::{App, AppError, WindowOptions};
pub use rouge_core as core;
pub use rouge_core::{
    Appearance, Decorations, EditAction, Menu, MenuItem, ResizeEdge, WindowHandle, appearance,
    window_handle,
};
pub use rouge_reactive as reactive;
pub use rouge_render as render;
pub use rouge_widgets as widgets;

pub mod prelude {
    pub use crate::app::{App, WindowOptions};
    pub use rouge_core::{
        Align, AnyView, Appearance, BackdropBlur, BlendMode, Blur, BoxShadow, Chord, Color,
        ColorMatrix, Corners, Cursor, Decay, Decorations, Div, DragEvent, DragPhase, Ease, Edges,
        EditAction, Fit, FocusEvent, FocusHandle, Glass, ImageData, ImageSource, ImeEvent, Inputs,
        Key, KeyEvent, Keyframes, LayoutTransition, Length, Mask, Menu, MenuItem, Modifiers,
        Motion, Painter, Palette, Path, Point, PointerButton, PointerEvent, Pose, Rect, ResizeEdge,
        RowKey, ScrollEvent, ScrollState, Scrollable, Size, SpanStyle, Spring, Style, Styled, Text,
        TextAlign, TextStyle, Theme, Tween, Underline, View, VirtualListHandle, WindowHandle,
        appearance, auto, canvas, div, dynamic, each, editable, focus_handle, icon, image, motion,
        overlay, pct, provide_theme, px, rem, rich_text, root_font_size, scroll, show, text,
        use_binding, use_theme, virtual_list, virtual_list_handle, window_handle, window_size,
    };
    #[cfg(feature = "tokio-rt")]
    pub use rouge_reactive::spawn_on;
    pub use rouge_reactive::{
        Effect, Memo, ReadSignal, Resource, Runtime, Scope, Signal, Task, TaskError, background,
        batch, effect, expect_context, memo, on_cleanup, provide_context, resource, signal, sleep,
        spawn, untrack, use_context,
    };
    pub use rouge_widgets::{
        MenuFocus, Placement, Side, button, checkbox, context_menu, menu_bar, menu_item, modal,
        popover, progress, radio, resizable_panel, scroll_view, select, separator, slider, spinner,
        tabs, text_area, text_input, toggle, tooltip, window_chrome,
    };
}
