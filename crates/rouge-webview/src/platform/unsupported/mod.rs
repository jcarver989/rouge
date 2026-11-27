use super::SystemEngine;
use crate::engine::{EnginePage, WebEngine, WebViewEvents, WebViewOptions};
use crate::error::WebViewError;

impl<T: 'static> WebEngine for SystemEngine<T> {
    fn open(
        &self,
        _options: WebViewOptions,
        _events: WebViewEvents,
    ) -> Result<Box<dyn EnginePage>, WebViewError> {
        Err(WebViewError::Unsupported)
    }
}
