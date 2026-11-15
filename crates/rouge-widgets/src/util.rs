use rouge_core::Role;
use rouge_core::{Div, Prop, Styled};
use std::rc::Rc;

pub(crate) fn fade_when_disabled(root: Div, disabled: &Rc<Prop<bool>>) -> Div {
    let disabled = disabled.clone();
    root.opacity(move || {
        if disabled.get() {
            crate::tokens::DISABLED_OPACITY
        } else {
            1.0
        }
    })
}

pub(crate) fn focus_ring(root: Div) -> Div {
    root.focus_visible(|style| style.border_color(Role::Accent))
}
