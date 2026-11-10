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
    for (edge, x, y) in [
        (ResizeEdge::North, 0, -1),
        (ResizeEdge::South, 0, 1),
        (ResizeEdge::West, -1, 0),
        (ResizeEdge::East, 1, 0),
        (ResizeEdge::NorthWest, -1, -1),
        (ResizeEdge::NorthEast, 1, -1),
        (ResizeEdge::SouthWest, -1, 1),
        (ResizeEdge::SouthEast, 1, 1),
    ] {
        let zone = div()
            .test_id(format!("window-resize-{edge:?}"))
            .absolute()
            .on_pointer_down(move |event| {
                event.stop_propagation();
                window.start_resize(edge);
            });
        let zone = match x {
            -1 => zone.left(0.0).w(4.0),
            1 => zone.right(0.0).w(4.0),
            _ => zone.left(4.0).right(4.0),
        };
        let zone = match y {
            -1 => zone.top(0.0).h(4.0),
            1 => zone.bottom(0.0).h(4.0),
            _ => zone.top(4.0).bottom(4.0),
        };
        chrome = chrome.child(zone);
    }
    chrome
}
