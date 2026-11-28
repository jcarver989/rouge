mod content;
mod engine;
mod error;
mod mounts;
mod page;
mod platform;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
mod view;

pub use content::WebContent;
pub use engine::{
    EnginePage, Layout, PageCommand, PageScale, Placement, WebEngine, WebViewEvent, WebViewEvents,
    WebViewOptions, provide_web_engine,
};
pub use error::WebViewError;
pub use page::{PageState, WebPage, web_page};
pub use platform::SystemEngine;
pub use view::{WebView, web_view};
