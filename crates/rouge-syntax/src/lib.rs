//! Portable Tree-sitter syntax highlighting and symbol tags.
mod cell;
mod error;
mod highlight;
mod incremental;
mod language;
mod spans;
mod symbols;
mod theme;

pub use cell::SyntaxCell;
pub use error::SyntaxError;
pub use highlight::{
    DocumentHighlights, HighlightLine, HighlightSpans, MAX_SOURCE_BYTES, SourceSyntax,
    SyntaxStream, SyntaxStreamUpdate, analyze, thread_work,
};
pub use incremental::SyntaxWorkStats;
pub use language::{LanguageHint, resolve_language};
pub use rouge_theme::Rgba;
pub use symbols::{
    LineSpan, SymbolDefinition, SymbolKind, SymbolReference, SymbolTags, has_symbols,
};
pub use theme::{FontStyle, HighlightId, HighlightSpan, SyntaxStyle, SyntaxTheme};
