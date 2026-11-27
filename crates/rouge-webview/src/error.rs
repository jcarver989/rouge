use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum WebViewError {
    #[error("this window lends no native handle to host a web view")]
    NoWindow,
    #[error("no system web view is available on this platform")]
    Unsupported,
    #[error("the system web view could not be created: {0}")]
    Create(String),
}
