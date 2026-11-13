//! Renderer-neutral Markdown documents and append streams.
mod document;
mod incremental;
mod stream;

pub use document::*;
pub use incremental::MarkdownParseStats;
pub use stream::{
    MarkdownStream, MarkdownStreamChanges, MarkdownStreamIdentity, MarkdownStreamUpdate,
};
