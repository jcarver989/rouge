use rouge_core::{
    Div, IntoProp, PointerEvent, ResizeEdge, Styled, View, WindowHandle, div, text, use_theme,
    window_handle,
};

pub fn window_chrome<T, U>(title: impl IntoProp<String, U>, root: T) -> Div
where
    T: View,
{
    let window = window_handle();
    let theme = use_theme();
    let button = move |id, label, click: fn(WindowHandle)| {
        div()
            .test_id(id)
            .w(40.0)
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .hover(move |s| s.bg(theme.hover.get_untracked()))
            .on_pointer_down(PointerEvent::stop_propagation)
            .on_click(move |_| click(window))
            .child(text(label))
    };
    let bar = div()
        .preserve_focus()
        .test_id("window-titlebar")
        .h(36.0)
        .flex_none()
        .flex()
        .items_center()
        .bg(theme.surface)
        .text_color(theme.text)
        .on_pointer_down(move |event| {
            if event.click_count == 2 {
                window.zoom();
            } else {
                window.start_drag();
            }
        })
        .child(div().flex_1().px(12.0).child(text(title)))
        .child(button("window-minimize", "−", WindowHandle::minimize))
        .child(button("window-zoom", "□", WindowHandle::zoom))
        .child(button("window-close", "×", WindowHandle::close));
    let mut chrome = div().relative().flex().col().size_full().child(bar).child(
        div()
            .test_id("window-content")
            .flex_1()
            .min_h(0.0)
            .child(root),
    );
    for (edge, top, right, bottom, left, width, height) in [
        (
            ResizeEdge::North,
            Some(0.0),
            Some(4.0),
            None,
            Some(4.0),
            None,
            Some(4.0),
        ),
        (
            ResizeEdge::South,
            None,
            Some(4.0),
            Some(0.0),
            Some(4.0),
            None,
            Some(4.0),
        ),
        (
            ResizeEdge::West,
            Some(4.0),
            None,
            Some(4.0),
            Some(0.0),
            Some(4.0),
            None,
        ),
        (
            ResizeEdge::East,
            Some(4.0),
            Some(0.0),
            Some(4.0),
            None,
            Some(4.0),
            None,
        ),
        (
            ResizeEdge::NorthWest,
            Some(0.0),
            None,
            None,
            Some(0.0),
            Some(4.0),
            Some(4.0),
        ),
        (
            ResizeEdge::NorthEast,
            Some(0.0),
            Some(0.0),
            None,
            None,
            Some(4.0),
            Some(4.0),
        ),
        (
            ResizeEdge::SouthWest,
            None,
            None,
            Some(0.0),
            Some(0.0),
            Some(4.0),
            Some(4.0),
        ),
        (
            ResizeEdge::SouthEast,
            None,
            Some(0.0),
            Some(0.0),
            None,
            Some(4.0),
            Some(4.0),
        ),
    ] {
        let mut zone = div()
            .test_id(format!("window-resize-{edge:?}"))
            .absolute()
            .on_pointer_down(move |event| {
                event.stop_propagation();
                window.start_resize(edge);
            });
        if let Some(value) = top {
            zone = zone.top(value);
        }
        if let Some(value) = right {
            zone = zone.right(value);
        }
        if let Some(value) = bottom {
            zone = zone.bottom(value);
        }
        if let Some(value) = left {
            zone = zone.left(value);
        }
        if let Some(value) = width {
            zone = zone.w(value);
        }
        if let Some(value) = height {
            zone = zone.h(value);
        }
        chrome = chrome.child(zone);
    }
    chrome
}
