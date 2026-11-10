mod app;
pub mod platform;

pub use app::{App, AppError, WindowOptions};
pub use rouge_core as core;
pub use rouge_reactive as reactive;
pub use rouge_render as render;
pub use rouge_widgets as widgets;

pub mod prelude {
    pub use crate::app::{App, WindowOptions};
    pub use rouge_core::{
        AnyView, BackdropBlur, BlendMode, Blur, BoxShadow, Chord, Color, ColorMatrix, Corners,
        Cursor, Decay, Div, DragEvent, DragPhase, Ease, Edges, Fit, FocusEvent, FocusHandle, Glass,
        ImageData, ImageSource, ImeEvent, Inputs, Key, KeyEvent, Keyframes, LayoutTransition,
        Length, Mask, Modifiers, Motion, Painter, Palette, Path, Point, PointerButton,
        PointerEvent, Pose, Rect, ScrollEvent, ScrollState, Scrollable, Size, SpanStyle, Spring,
        Style, Styled, Text, TextAlign, TextStyle, Theme, Tween, Underline, View, auto, canvas,
        div, dynamic, each, editable, focus_handle, icon, image, motion, overlay, pct,
        provide_theme, px, rich_text, scroll, show, text, use_theme, virtual_list, window_size,
    };
    pub use rouge_reactive::{
        Effect, Memo, ReadSignal, Resource, Runtime, Scope, Signal, Task, background, batch,
        effect, expect_context, memo, on_cleanup, provide_context, resource, signal, sleep, spawn,
        untrack, use_context,
    };
    pub use rouge_widgets::{
        Placement, button, checkbox, context_menu, menu_item, modal, popover, progress, radio,
        scroll_view, select, separator, slider, spinner, tabs, text_area, text_input, toggle,
        tooltip,
    };
}
