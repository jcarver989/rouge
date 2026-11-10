use crate::util::{either, ring, themed};
use rouge_core::{
    AnyView, BuildCx, Color, Cursor, Div, ElementId, Key, Styled, View, div, dynamic, styled, text,
    use_theme,
};
use rouge_reactive::Signal;
use std::rc::Rc;

type Content = Rc<dyn Fn() -> AnyView>;

pub struct Tabs {
    root: Div,
    selected: Signal<usize>,
    #[expect(clippy::struct_field_names)]
    tabs: Vec<(String, Content)>,
}

pub fn tabs(selected: Signal<usize>) -> Tabs {
    Tabs {
        root: div().flex().col(),
        selected,
        tabs: Vec::new(),
    }
}

impl Tabs {
    pub fn tab<T: View>(
        mut self,
        label: impl Into<String>,
        content: impl Fn() -> T + 'static,
    ) -> Self {
        self.tabs
            .push((label.into(), Rc::new(move || content().into_any())));
        self
    }
}

styled!(Tabs => root);

impl View for Tabs {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, selected) = (use_theme(), self.selected);
        let count = self.tabs.len().max(1);
        let (labels, contents): (Vec<_>, Vec<_>) = self.tabs.into_iter().unzip();
        let headers = labels.into_iter().enumerate().map(move |(index, label)| {
            let on = move || selected.get() == index;
            let color = either(on, theme.text, theme.text_muted);
            let caption = themed(text(label), color, theme).no_wrap();
            div()
                .px(12.0)
                .py(8.0)
                .border_b(2.0)
                .border_color(move || {
                    if on() {
                        theme.accent.get()
                    } else {
                        Color::TRANSPARENT
                    }
                })
                .focusable()
                .cursor(Cursor::Pointer)
                .focus_visible(ring(theme))
                .on_click(move |_| selected.set(index))
                .on_key_down(move |event| {
                    let current = selected.get_untracked().min(count - 1);
                    match event.key {
                        Key::ArrowRight => selected.set((current + 1) % count),
                        Key::ArrowLeft => selected.set((current + count - 1) % count),
                        _ => return,
                    }
                    event.stop_propagation();
                })
                .child(caption)
        });
        let strip = div()
            .flex()
            .flex_none()
            .border_b(1.0)
            .border_color(theme.border)
            .children(headers);
        let body = dynamic(move || match contents.get(selected.get()) {
            Some(content) => content(),
            None => div().into_any(),
        });
        self.root
            .child(strip)
            .child(div().flex_1().min_h(0.0).child(body))
            .build(cx)
    }
}
