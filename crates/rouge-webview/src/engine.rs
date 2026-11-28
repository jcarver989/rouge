use crate::content::WebContent;
use crate::error::WebViewError;
use rouge_core::{Corners, PaintRegion, Rect, Size};
use rouge_reactive::provide_context;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WebViewOptions {
    pub inspectable: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    pub frame: Rect,
    pub clip: Rect,
    pub corners: Corners,
    pub device_scale: f32,
    pub page_zoom: f32,
    pub interactive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub clip: Rect,
    pub page: Rect,
    pub unzoomed: Size,
    pub corners: Corners,
    pub zoom: f32,
    pub interactive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageScale {
    Zoom(f32),
    Fit(Size),
}

impl Placement {
    pub fn from_region(
        region: &PaintRegion,
        window_zoom: f32,
        device_scale: f32,
        scale: PageScale,
        interactive: bool,
    ) -> Self {
        let frame = region.frame.scale(window_zoom);
        let (frame, page_zoom) = match scale {
            PageScale::Zoom(zoom) => (frame, zoom * window_zoom),
            PageScale::Fit(size) if size.width > 0.0 => {
                let zoom = frame.size.width / size.width;
                let fitted = Size::new(frame.size.width, size.height * zoom);
                (
                    Rect {
                        origin: frame.origin,
                        size: fitted,
                    },
                    zoom,
                )
            }
            PageScale::Fit(_) => (frame, window_zoom),
        };
        Placement {
            frame,
            clip: region.bounds.scale(window_zoom),
            corners: region.corners.map(|radius| radius * window_zoom),
            device_scale,
            page_zoom,
            interactive,
        }
    }

    pub fn visible(&self) -> bool {
        !self.clip.is_empty()
    }

    pub fn layout(&self, unit: f32) -> Option<Layout> {
        let (frame, clip) = (self.frame, self.clip);
        let (left, top) = (
            clip.origin.x <= frame.origin.x,
            clip.origin.y <= frame.origin.y,
        );
        let (right, bottom) = (clip.max().x >= frame.max().x, clip.max().y >= frame.max().y);
        let keep = |kept: bool, radius: f32| if kept { radius * unit } else { 0.0 };
        let zoom = self.page_zoom.max(MIN_ZOOM);
        let page = Rect {
            origin: frame.origin - clip.origin,
            size: frame.size,
        }
        .scale(unit);
        self.visible().then(|| Layout {
            clip: clip.scale(unit),
            page,
            unzoomed: Size::new(
                (page.size.width / zoom).round(),
                (page.size.height / zoom).round(),
            ),
            corners: Corners {
                top_left: keep(left && top, self.corners.top_left),
                top_right: keep(right && top, self.corners.top_right),
                bottom_right: keep(right && bottom, self.corners.bottom_right),
                bottom_left: keep(left && bottom, self.corners.bottom_left),
            },
            zoom,
            interactive: self.interactive,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PageCommand {
    Load(WebContent),
    Place(Placement),
    Reload,
    Stop,
    Back,
    Forward,
    Post(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum WebViewEvent {
    Loading(bool),
    Progress(f32),
    Url(String),
    Title(String),
    History { back: bool, forward: bool },
    Failed(String),
    Message(String),
}

#[derive(Clone)]
pub struct WebViewEvents(Arc<dyn Fn(WebViewEvent) + Send + Sync>);

impl WebViewEvents {
    pub fn new(sink: impl Fn(WebViewEvent) + Send + Sync + 'static) -> Self {
        WebViewEvents(Arc::new(sink))
    }

    pub fn send(&self, event: WebViewEvent) {
        (self.0)(event);
    }
}

pub trait EnginePage {
    fn apply(&mut self, command: PageCommand);
}

pub trait WebEngine: 'static {
    fn open(
        &self,
        options: WebViewOptions,
        events: WebViewEvents,
    ) -> Result<Box<dyn EnginePage>, WebViewError>;
}

pub fn provide_web_engine(engine: impl WebEngine) {
    provide_context(Rc::new(engine) as Rc<dyn WebEngine>);
}

const MIN_ZOOM: f32 = 0.01;
