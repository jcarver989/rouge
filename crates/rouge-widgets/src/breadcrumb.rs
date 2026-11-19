use crate::icon::{Icon, IconName};
use crate::tokens::{ICON_BUTTON_SIZE, ICON_SIZE_SMALL, Radius, Space};
use crate::typography::UiStyles;
use crate::util::{TestId, chrome, focus_ring};
use rouge_core::{
    Apply, BuildCx, Color, DelegateStyle, Div, Role, StyleDelta, Styled, View, component, div,
    text, view,
};
use rouge_reactive::Callback;

pub struct Breadcrumb {
    links: Vec<(String, Callback)>,
    current: Option<String>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn breadcrumb() -> Breadcrumb {
    Breadcrumb {
        links: Vec::new(),
        current: None,
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Breadcrumb {
    pub fn link(mut self, label: impl Into<String>, on_click: impl Into<Callback>) -> Self {
        self.links.push((label.into(), on_click.into()));
        self
    }

    pub fn current(mut self, label: impl Into<String>) -> Self {
        self.current = Some(label.into());
        self
    }
}

chrome!(Breadcrumb);

impl View for Breadcrumb {
    fn build(self, cx: &mut BuildCx<'_>) {
        let test_id = self.test_id;
        let crumb = || {
            view! {
                <div flex items_center min_h=ICON_BUTTON_SIZE px=Space::X1 rounded=Radius::Inner border=1.0
                     border_color=Color::TRANSPARENT/>
            }
        };
        let arrow = || Icon::new(IconName::ChevronRight, Role::TextMuted).size(ICON_SIZE_SMALL);
        let mut parts = Vec::new();
        for (index, (label, on_click)) in self.links.into_iter().enumerate() {
            if index > 0 {
                parts.push(arrow().into_any());
            }
            let link_id = test_id.part(format_args!("{index}"));
            let link = focus_ring(crumb())
                .text_color(Role::TextMuted)
                .hover(|style| style.bg(Role::SurfaceHover).text_color(Role::Text))
                .on_activate(move || on_click.call())
                .when_some(link_id, Div::test_id)
                .child(view! { <text body no_wrap>{label}</text> });
            parts.push(link.into_any());
        }
        if let Some(current) = self.current {
            if !parts.is_empty() {
                parts.push(arrow().into_any());
            }
            let here = crumb()
                .min_w(0.0)
                .overflow_hidden()
                .text_color(Role::Text)
                .when_some(test_id.part("current"), Div::test_id)
                .child(view! { <text body weight=500.0 no_wrap>{current}</text> });
            parts.push(here.into_any());
        }
        let root = view! { <div flex items_center gap=Space::X1 test_id?=test_id>{..parts}</div> };
        self.style.apply(root).build(cx);
    }
}
