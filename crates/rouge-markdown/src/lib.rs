//! Renderer-neutral Markdown documents and append streams.
mod document;
mod stream;
mod text;

pub use document::*;
pub use stream::{
    MarkdownParseStats, MarkdownStream, MarkdownStreamChanges, MarkdownStreamIdentity,
    MarkdownStreamUpdate,
};
pub use text::{MarkdownLinkRange, MarkdownMarkRange, MarkdownMarks, MarkdownText};
