use crate::engine::Placement;
use crate::error::WebViewError;
use crate::page::WebPage;
use rouge_core::access::Role;
use rouge_core::{
    AnyView, BuildCx, DelegateStyle, Div, FocusHandle, IntoProp, PaintRegion, Prop, Styled, View,
    component, device_scale, div, zoom,
};
use rouge_reactive::signal;

type Fallback = Box<dyn FnOnce(WebViewError) -> AnyView>;

pub struct WebView {
    div: Div,
    page: WebPage,
    interactive: Prop<bool>,
    page_zoom: Prop<f32>,
    fallback: Option<Fallback>,
}

#[component]
pub fn web_view(page: WebPage) -> WebView {
    WebView {
        div: div(),
        page,
        interactive: Prop::Static(true),
        page_zoom: Prop::Static(1.0),
        fallback: None,
    }
}

impl WebView {
    pub fn interactive<T>(mut self, interactive: impl IntoProp<bool, T>) -> Self {
        self.interactive = interactive.into_prop();
        self
    }

    pub fn page_zoom<T>(mut self, zoom: impl IntoProp<f32, T>) -> Self {
        self.page_zoom = zoom.into_prop();
        self
    }

    pub fn fallback<T: View>(mut self, view: impl FnOnce(WebViewError) -> T + 'static) -> Self {
        self.fallback = Some(Box::new(move |error| view(error).into_any()));
        self
    }

    pub fn track_focus(mut self, focus: FocusHandle) -> Self {
        self.div = self.div.focusable().track_focus(focus);
        self
    }

    pub fn access_label(mut self, label: impl Into<String>) -> Self {
        self.div = self.div.role(Role::Group).access_label(label.into());
        self
    }

    pub fn test_id(mut self, name: impl Into<String>) -> Self {
        self.div = self.div.test_id(name.into());
        self
    }
}

impl DelegateStyle for WebView {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.div
    }
}

impl View for WebView {
    fn build(self, cx: &mut BuildCx<'_>) {
        let page = self.page;
        if let Some(error) = page.error() {
            self.div
                .child(self.fallback.map(|fallback| fallback(error)))
                .build(cx);
            return;
        }
        let region = signal(PaintRegion::default());
        let (window_zoom, device_scale) = (zoom(), device_scale());
        page.attach(move || {
            region.with(|region| {
                Placement::from_region(
                    region,
                    window_zoom.get(),
                    device_scale.get(),
                    self.page_zoom.get(),
                    self.interactive.get(),
                )
            })
        });
        self.div.track_paint_region(region).build(cx);
    }
}
