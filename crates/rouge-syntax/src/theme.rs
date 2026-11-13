use arborium_theme::{HIGHLIGHTS, tag_to_name};
use rouge_theme::Rgba;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Range, sync::Arc};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FontStyle {
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
}

impl FontStyle {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
        }
    }

    #[must_use]
    pub const fn is_plain(self) -> bool {
        !self.bold && !self.italic && !self.underline
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HighlightId(u8);

impl HighlightId {
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        tag_to_name(tag)?;
        let index = HIGHLIGHTS
            .iter()
            .position(|highlight| highlight.tag == tag)?;
        u8::try_from(index).ok().map(Self)
    }
}

/// A highlighted, UTF-8-safe byte range into the supplied source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub highlight: HighlightId,
}

/// Style assigned to a Tree-sitter capture name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxStyle {
    pub foreground: Rgba,
    #[serde(default)]
    pub font_style: FontStyle,
}

/// Styles keyed by Tree-sitter capture name.
///
/// Capture lookup first tries an exact name and then progressively removes
/// dot-separated suffixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxTheme {
    captures: BTreeMap<String, SyntaxStyle>,
    highlights: Arc<[Option<SyntaxStyle>]>,
}

impl SyntaxTheme {
    #[must_use]
    pub fn new(captures: BTreeMap<String, SyntaxStyle>) -> Self {
        let highlights = HIGHLIGHTS
            .iter()
            .map(|highlight| {
                tag_to_name(highlight.tag)
                    .and_then(|name| lookup(&captures, capture_for_name(name)))
            })
            .collect();
        Self {
            captures,
            highlights,
        }
    }

    #[must_use]
    pub const fn captures(&self) -> &BTreeMap<String, SyntaxStyle> {
        &self.captures
    }

    #[must_use]
    pub fn style(&self, capture: &str) -> Option<SyntaxStyle> {
        lookup(&self.captures, capture)
    }

    #[must_use]
    pub fn highlight(&self, id: HighlightId) -> Option<SyntaxStyle> {
        self.highlights.get(usize::from(id.0)).copied().flatten()
    }
}

impl Default for SyntaxTheme {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}

fn capture_for_name(name: &'static str) -> &'static str {
    match name {
        "title" => "markup.heading",
        "strong" => "markup.bold",
        "emphasis" => "markup.italic",
        "link" => "markup.link",
        "literal" => "markup.raw",
        "strikethrough" => "markup.strikethrough",
        name => name,
    }
}

fn lookup(captures: &BTreeMap<String, SyntaxStyle>, capture: &str) -> Option<SyntaxStyle> {
    let mut candidate = capture;
    loop {
        if let Some(style) = captures.get(candidate) {
            return Some(*style);
        }
        let (parent, _) = candidate.rsplit_once('.')?;
        candidate = parent;
    }
}
