use rouge_core::{Cursor, Div, Interactions, Key, KeyEvent, Styled, Theme};
use rouge_reactive::Callback;
use rouge_theme::InteractionState;

pub(crate) fn activates(event: &KeyEvent) -> bool {
    matches!(event.key, Key::Enter | Key::Space)
}

pub(crate) fn interaction(state: Interactions, enabled: bool) -> InteractionState {
    if !enabled {
        InteractionState::Disabled
    } else if state.hovered {
        InteractionState::Hovered
    } else {
        InteractionState::Rest
    }
}

pub(crate) fn focus_ring(root: Div) -> Div {
    root.focus_visible(|style| style.border_color(Theme::current().accent()))
}

pub(crate) fn pressable(root: Div, keyboard: bool, enabled: bool, action: Callback) -> Div {
    let root = root.enabled(enabled);
    if !enabled || !action.is_set() {
        return root;
    }
    if keyboard {
        root.on_press(move || action.call())
    } else {
        root.cursor(Cursor::Pointer).on_click(move |event| {
            event.stop_propagation();
            action.call();
        })
    }
}
