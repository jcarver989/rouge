use crate::tokens::Space;
use rouge_core::Role;
use rouge_core::access;
use rouge_core::view;
use rouge_core::{
    Div, IntoProp, PointerButton, PointerEvent, ResizeEdge, Styled, View, WindowHandle, component,
    div, text, window_handle,
};

#[component]
pub fn window_chrome<T, U>(title: impl IntoProp<String, U>, children: T) -> Div
where
    T: View,
{
    let window = window_handle();
    let button = move |id: &str, name: &str, label: &str, click: fn(WindowHandle)| {
        view! {
            <div test_id=id role=access::Role::Button access_label={name.to_owned()}
                 w=40.0 h_full flex items_center justify_center hover:bg=Role::SurfaceHover
                 on_pointer_down=PointerEvent::stop_propagation on_click={move |_| click(window)}>
                <text>{label}</text>
            </div>
        }
    };
    let bar = view! {
        <div preserve_focus test_id="window-titlebar" h=36.0 flex_none flex items_center
             bg=Role::Elevated text_color=Role::Text
             on_pointer_down={move |event| match (event.button, event.click_count) {
                 (PointerButton::Primary, 2) => window.zoom(),
                 (PointerButton::Primary, _) => window.start_drag(),
                 _ => {}
             }}>
            <div flex_1 px=Space::X3>
                <text>{title}</text>
            </div>
            {button("window-minimize", "Minimize", "−", WindowHandle::minimize)}
            {button("window-zoom", "Zoom", "□", WindowHandle::zoom)}
            {button("window-close", "Close", "×", WindowHandle::close)}
        </div>
    };
    let mut chrome = view! {
        <div relative flex col size_full>
            {bar}
            <div test_id="window-content" root_host flex_1 min_h=0.0>{children}</div>
        </div>
    };
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
        let zone = view! {
            <div absolute test_id={format!("window-resize-{edge:?}")} on_pointer_down={move |event| {
                event.stop_propagation();
                window.start_resize(edge);
            }}/>
        };
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
