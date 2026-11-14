//! Renderer-neutral Markdown documents and append streams.
mod document;
mod incremental;
mod stream;
mod text;

pub use document::*;
pub use incremental::MarkdownParseStats;
pub use stream::{
    MarkdownStream, MarkdownStreamChanges, MarkdownStreamIdentity, MarkdownStreamUpdate,
};
pub use text::{MarkdownLinkRange, MarkdownMarkRange, MarkdownMarks, MarkdownText};
