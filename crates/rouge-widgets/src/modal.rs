use crate::popover::{panel, popup};
use rouge_core::{AnyView, BuildCx, Div, ElementId, ScrollEvent, Styled, View, use_theme};
use rouge_reactive::Signal;

pub struct Modal {
    open: Signal<bool>,
    content: Box<dyn Fn() -> AnyView>,
    dismissable: bool,
}

pub fn modal<T: View>(open: Signal<bool>, content: impl Fn() -> T + 'static) -> Modal {
    Modal {
        open,
        content: Box::new(move || content().into_any()),
        dismissable: true,
    }
}

impl Modal {
    pub fn dismissable(mut self, dismissable: bool) -> Self {
        self.dismissable = dismissable;
        self
    }
}

impl View for Modal {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let theme = use_theme();
        let content = self.content;
        let dialog = move || {
            panel(theme)
                .p(16.0)
                .focus_scope()
                .autofocus()
                .child(content())
        };
        let scrim = move |layer: Div| {
            layer
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.scrim)
                .on_scroll(ScrollEvent::stop_propagation)
        };
        popup(self.open, self.dismissable, scrim, dialog).build(cx)
    }
}
