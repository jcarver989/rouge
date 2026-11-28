#[cfg_attr(target_os = "macos", path = "macos/mod.rs")]
#[cfg_attr(target_os = "ios", path = "ios/mod.rs")]
#[allow(unsafe_code)]
mod host;
#[allow(unsafe_code)]
mod webkit;
#[allow(unsafe_code)]
mod wk_web_view;

use super::SystemEngine;
use super::shared::inline::Inline;
use crate::content::WebContent;
use crate::engine::{
    EnginePage, PageCommand, Placement, WebEngine, WebViewEvent, WebViewEvents, WebViewOptions,
};
use crate::error::WebViewError;
use host::Clip;
use objc2::MainThreadMarker;
use objc2_core_foundation::CGFloat;
use objc2_quartz_core::{CACornerMask, CALayer};
use raw_window_handle::HasWindowHandle;
use rouge_core::Corners;
use webkit::WebKit;

impl<T: HasWindowHandle + 'static> WebEngine for SystemEngine<T> {
    fn open(
        &self,
        options: WebViewOptions,
        events: WebViewEvents,
    ) -> Result<Box<dyn EnginePage>, WebViewError> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| WebViewError::Create("web views open on the main thread".into()))?;
        let window = self.0.window_handle().map_err(|_| WebViewError::NoWindow)?;
        let parent = host::parent(window.as_raw()).ok_or(WebViewError::NoWindow)?;
        let web = WebKit::open(mtm, options, events.clone());
        Ok(Box::new(Page {
            clip: Clip::new(mtm, parent, web.view()),
            web,
            events,
            interactive: true,
        }))
    }
}

struct Page {
    clip: Clip,
    web: WebKit,
    events: WebViewEvents,
    interactive: bool,
}

impl Page {
    fn load(&self, content: &WebContent) {
        let loaded = match content {
            WebContent::Url(url) => self.web.load_url(url),
            WebContent::Html { html, base_url } => {
                self.web.load_inline(Inline::new(html, base_url.as_deref()))
            }
        };
        if let (false, WebContent::Url(url)) = (loaded, content) {
            self.events
                .send(WebViewEvent::Failed(format!("{url} is not a URL")));
        }
    }

    fn place(&mut self, placement: &Placement) {
        if self.interactive != placement.interactive {
            self.interactive = placement.interactive;
            self.clip
                .set_interactive(self.web.view(), placement.interactive);
        }
        match placement.layout(1.0) {
            Some(layout) => self.clip.place(self.web.view(), &layout, placement.opacity),
            None => self.clip.hide(),
        }
    }
}

impl EnginePage for Page {
    fn apply(&mut self, command: PageCommand) {
        match command {
            PageCommand::Load(content) => self.load(&content),
            PageCommand::Place(placement) => self.place(&placement),
            command => self.web.run(command),
        }
    }
}

fn round(layer: &CALayer, corners: Corners) {
    let radii = [
        (corners.top_left, CACornerMask::LayerMinXMinYCorner),
        (corners.top_right, CACornerMask::LayerMaxXMinYCorner),
        (corners.bottom_right, CACornerMask::LayerMaxXMaxYCorner),
        (corners.bottom_left, CACornerMask::LayerMinXMaxYCorner),
    ];
    let radius = radii
        .iter()
        .fold(0.0_f32, |most, (radius, _)| most.max(*radius));
    let mask = radii
        .iter()
        .filter(|(radius, _)| *radius > 0.0)
        .fold(CACornerMask::empty(), |mask, (_, corner)| mask | *corner);
    layer.setMasksToBounds(true);
    layer.setCornerRadius(CGFloat::from(radius));
    layer.setMaskedCorners(mask);
}
