use crate::util::{pressable, themed, tint};
use rouge_core::{
    AnyView, BuildCx, Color, Div, ElementId, IntoProp, Prop, Styled, Text, Theme, View, div,
    styled, text, use_theme,
};
use rouge_reactive::Signal;
use std::rc::Rc;

pub struct Button {
    root: Div,
    content: Result<Text, AnyView>,
    on_click: Box<dyn Fn()>,
    enabled: Prop<bool>,
    color: Signal<Color>,
    theme: Theme,
}

#[track_caller]
pub fn button<U>(label: impl IntoProp<String, U>) -> Button {
    let theme = use_theme();
    let root = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .px(12.0)
        .py(6.0)
        .border(1.0)
        .border_color(theme.border)
        .rounded(theme.radius)
        .bg(theme.surface)
        .hover(tint(theme.surface, theme.hover))
        .active(tint(theme.surface, theme.pressed));
    Button {
        root,
        content: Ok(text(label)),
        on_click: Box::new(|| ()),
        enabled: Prop::Static(true),
        color: theme.text,
        theme,
    }
}

impl Button {
    pub fn primary(mut self) -> Self {
        let theme = self.theme;
        self.color = theme.accent_text;
        self.root = self
            .root
            .bg(theme.accent)
            .border_color(theme.accent)
            .hover(tint(theme.accent, theme.hover))
            .active(tint(theme.accent, theme.pressed));
        self
    }

    pub fn content(mut self, content: impl View) -> Self {
        self.content = Err(content.into_any());
        self
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Box::new(f);
        self
    }

    pub fn enabled<U>(mut self, enabled: impl IntoProp<bool, U>) -> Self {
        self.enabled = enabled.into_prop();
        self
    }
}

styled!(Button => root);

impl View for Button {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let content = match self.content {
            Ok(label) => themed(label, self.color, self.theme).no_wrap().into_any(),
            Err(view) => view,
        };
        let enabled = Rc::new(self.enabled);
        let lit = enabled.clone();
        pressable(self.root, self.theme, self.on_click)
            .enabled(move || enabled.get())
            .opacity(move || if lit.get() { 1.0 } else { 0.45 })
            .child(content)
            .build(cx)
    }
}
