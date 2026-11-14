use crate::document::MarkdownInline;
use serde::{Deserialize, Serialize};
use std::ops::Range;

const OPENABLE_SCHEMES: [&str; 3] = ["https://", "http://", "mailto:"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarkdownMarks(u8);

impl MarkdownMarks {
    pub const NONE: Self = Self(0);
    pub const STRONG: Self = Self(1);
    pub const EMPHASIS: Self = Self(1 << 1);
    pub const STRIKETHROUGH: Self = Self(1 << 2);
    pub const CODE: Self = Self(1 << 3);
    pub const LINK: Self = Self(1 << 4);
    pub const IMAGE: Self = Self(1 << 5);

    #[must_use]
    pub const fn with(self, marks: Self) -> Self {
        Self(self.0 | marks.0)
    }

    #[must_use]
    pub const fn contains(self, marks: Self) -> bool {
        self.0 & marks.0 == marks.0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownMarkRange {
    pub range: Range<usize>,
    pub marks: MarkdownMarks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownLinkRange {
    pub range: Range<usize>,
    pub destination: String,
    pub title: Option<String>,
}

impl MarkdownLinkRange {
    #[must_use]
    pub fn openable(&self) -> bool {
        openable(&self.destination)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownText {
    pub(crate) text: String,
    marks: Vec<MarkdownMarkRange>,
    links: Vec<MarkdownLinkRange>,
}

impl MarkdownText {
    #[must_use]
    pub fn new(content: &[MarkdownInline]) -> Self {
        let mut text = Self::default();
        text.push_all(content, MarkdownMarks::NONE);
        text
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn marks(&self) -> &[MarkdownMarkRange] {
        &self.marks
    }

    #[must_use]
    pub fn links(&self) -> &[MarkdownLinkRange] {
        &self.links
    }

    #[must_use]
    pub fn link_at(&self, offset: usize) -> Option<&MarkdownLinkRange> {
        let index = self.links.partition_point(|link| link.range.end <= offset);
        self.links
            .get(index)
            .filter(|link| link.range.contains(&offset))
    }

    fn push_all(&mut self, content: &[MarkdownInline], marks: MarkdownMarks) {
        for inline in content {
            self.push(inline, marks);
        }
    }

    fn push(&mut self, inline: &MarkdownInline, marks: MarkdownMarks) {
        match inline {
            MarkdownInline::Text(text) => self.leaf(text, marks),
            MarkdownInline::Code(text) => self.leaf(text, marks.with(MarkdownMarks::CODE)),
            MarkdownInline::ImageAlt(text) => self.leaf(text, marks.with(MarkdownMarks::IMAGE)),
            MarkdownInline::SoftBreak => self.leaf(" ", marks),
            MarkdownInline::HardBreak => self.leaf("\n", marks),
            MarkdownInline::Strong(children) => {
                self.push_all(children, marks.with(MarkdownMarks::STRONG));
            }
            MarkdownInline::Emphasis(children) => {
                self.push_all(children, marks.with(MarkdownMarks::EMPHASIS));
            }
            MarkdownInline::Strikethrough(children) => {
                self.push_all(children, marks.with(MarkdownMarks::STRIKETHROUGH));
            }
            MarkdownInline::Link {
                destination,
                title,
                content,
            } => {
                let start = self.text.len();
                self.push_all(content, marks.with(MarkdownMarks::LINK));
                if self.text.len() > start {
                    self.links.push(MarkdownLinkRange {
                        range: start..self.text.len(),
                        destination: destination.clone(),
                        title: title.clone(),
                    });
                }
            }
        }
    }

    fn leaf(&mut self, text: &str, marks: MarkdownMarks) {
        let start = self.text.len();
        self.text.push_str(text);
        let end = self.text.len();
        if marks.is_empty() || start == end {
            return;
        }
        match self.marks.last_mut() {
            Some(last) if last.marks == marks && last.range.end == start => last.range.end = end,
            _ => self.marks.push(MarkdownMarkRange {
                range: start..end,
                marks,
            }),
        }
    }
}

pub(crate) fn openable(destination: &str) -> bool {
    OPENABLE_SCHEMES
        .iter()
        .any(|scheme| destination.starts_with(scheme))
}
