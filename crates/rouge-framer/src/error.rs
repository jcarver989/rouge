use base64::DecodeError;
use cargo_metadata::Error as MetadataError;
use framer::FramerError;
use plist::Error as PlistError;
use std::io;
use thiserror::Error;
use xshell::Error as ShellError;

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("{0}")]
    Invalid(String),

    #[error(transparent)]
    Config(#[from] FramerError),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Shell(#[from] ShellError),

    #[error(transparent)]
    Metadata(#[from] MetadataError),

    #[error(transparent)]
    Plist(#[from] PlistError),

    #[error("{name} is not valid base64: {source}")]
    Base64 { name: String, source: DecodeError },
}

pub(crate) type Result<T, E = PublishError> = std::result::Result<T, E>;

pub(crate) fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(PublishError::Invalid(message.into()))
}

pub(crate) fn ensure(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition { Ok(()) } else { invalid(message) }
}
