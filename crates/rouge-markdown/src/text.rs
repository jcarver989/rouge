use crate::document::Definition;
use linkify::{LinkFinder, LinkKind};
use pulldown_cmark::{Event, LinkType, Tag};
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
        OPENABLE_SCHEMES
            .iter()
            .any(|scheme| self.destination.starts_with(scheme))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownText {
    text: String,
    marks: Vec<MarkdownMarkRange>,
    links: Vec<MarkdownLinkRange>,
}

impl MarkdownText {
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

    fn link(&mut self, start: usize, destination: String, title: Option<String>) {
        if self.text.len() > start {
            self.links.push(MarkdownLinkRange {
                range: start..self.text.len(),
                destination,
                title,
            });
        }
    }
}

enum Open {
    Marks(MarkdownMarks),
    Label,
    Link {
        marks: MarkdownMarks,
        start: usize,
        destination: String,
        title: Option<String>,
    },
    Image(MarkdownMarks),
}

#[derive(Default)]
pub(crate) struct TextBuilder {
    text: MarkdownText,
    open: Vec<Open>,
    run: String,
    alt: Option<String>,
    links: usize,
    raw: bool,
}

impl TextBuilder {
    pub(crate) fn raw() -> Self {
        Self {
            raw: true,
            ..Self::default()
        }
    }

    fn marks(&self) -> MarkdownMarks {
        self.open
            .iter()
            .rev()
            .find_map(|open| match open {
                Open::Marks(marks) | Open::Image(marks) | Open::Link { marks, .. } => Some(*marks),
                Open::Label => None,
            })
            .unwrap_or_default()
    }

    pub(crate) fn start(
        &mut self,
        tag: &Tag<'_>,
        resolve: impl FnOnce(&str) -> Option<Definition>,
    ) {
        let marks = self.marks();
        if self.alt.is_some() {
            self.open.push(Open::Marks(marks));
            return;
        }
        let mut open = match tag {
            Tag::Strong => Open::Marks(marks.with(MarkdownMarks::STRONG)),
            Tag::Emphasis => Open::Marks(marks.with(MarkdownMarks::EMPHASIS)),
            Tag::Strikethrough => Open::Marks(marks.with(MarkdownMarks::STRIKETHROUGH)),
            Tag::Image { .. } => {
                self.alt = Some(String::new());
                Open::Image(marks.with(MarkdownMarks::IMAGE))
            }
            Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            } => {
                let resolved = is_reference(*link_type).then(|| resolve(id)).flatten();
                match resolved {
                    None if is_unknown(*link_type) => {
                        self.open.push(Open::Label);
                        return;
                    }
                    resolved => {
                        let (destination, title) = resolved.map_or_else(
                            || {
                                (
                                    dest_url.to_string(),
                                    (!title.is_empty()).then(|| title.to_string()),
                                )
                            },
                            |resolved| (resolved.destination, resolved.title),
                        );
                        self.links += 1;
                        Open::Link {
                            marks: marks.with(MarkdownMarks::LINK),
                            start: 0,
                            destination,
                            title,
                        }
                    }
                }
            }
            _ => Open::Marks(marks),
        };
        self.flush();
        if let Open::Link { start, .. } = &mut open {
            *start = self.text.text.len();
        }
        self.open.push(open);
    }

    pub(crate) fn end(&mut self) {
        let alt = self.alt.is_some();
        match self.open.last() {
            None => return,
            Some(Open::Label | Open::Image(_)) => {}
            Some(_) if alt => {}
            Some(_) => self.flush(),
        }
        match self.open.pop() {
            Some(Open::Link {
                start,
                destination,
                title,
                ..
            }) if !alt => {
                self.links -= 1;
                self.text.link(start, destination, title);
            }
            Some(Open::Image(marks)) => {
                let alt = self.alt.take().unwrap_or_default();
                self.text.leaf(&alt, marks);
            }
            _ => {}
        }
    }

    pub(crate) fn event(&mut self, event: &Event<'_>) {
        match event {
            Event::Text(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text)
            | Event::FootnoteReference(text)
            | Event::InlineHtml(text)
            | Event::Html(text) => self.push_text(text),
            Event::TaskListMarker(checked) => self.push_text(if *checked { "☑" } else { "☐" }),
            Event::Code(code) => self.leaf(code, MarkdownMarks::CODE),
            Event::SoftBreak => self.leaf(" ", MarkdownMarks::NONE),
            Event::HardBreak => self.leaf("\n", MarkdownMarks::NONE),
            Event::Start(_) | Event::End(_) | Event::Rule => {}
        }
    }

    fn push_text(&mut self, text: &str) {
        if let Some(alt) = &mut self.alt {
            alt.push_str(text);
        } else if self.raw || self.links > 0 {
            let marks = self.marks();
            self.text.leaf(text, marks);
        } else {
            self.run.push_str(text);
        }
    }

    fn leaf(&mut self, text: &str, marks: MarkdownMarks) {
        if let Some(alt) = &mut self.alt {
            alt.push_str(text);
            return;
        }
        self.flush();
        let marks = self.marks().with(marks);
        self.text.leaf(text, marks);
    }

    fn flush(&mut self) {
        if self.run.is_empty() {
            return;
        }
        let marks = self.marks();
        let run = std::mem::take(&mut self.run);
        let mut rest = 0;
        for link in LinkFinder::new().links(&run) {
            let destination = match link.kind() {
                LinkKind::Url
                    if OPENABLE_SCHEMES
                        .iter()
                        .any(|scheme| link.as_str().starts_with(scheme)) =>
                {
                    link.as_str().to_owned()
                }
                LinkKind::Email => format!("mailto:{}", link.as_str()),
                _ => continue,
            };
            self.text.leaf(&run[rest..link.start()], marks);
            let start = self.text.text.len();
            self.text
                .leaf(link.as_str(), marks.with(MarkdownMarks::LINK));
            self.text.link(start, destination, None);
            rest = link.end();
        }
        self.text.leaf(&run[rest..], marks);
        self.run = run;
        self.run.clear();
    }

    pub(crate) fn finish(mut self) -> MarkdownText {
        self.flush();
        self.text
    }
}

const fn is_unknown(link_type: LinkType) -> bool {
    matches!(
        link_type,
        LinkType::ReferenceUnknown | LinkType::CollapsedUnknown | LinkType::ShortcutUnknown
    )
}

const fn is_reference(link_type: LinkType) -> bool {
    is_unknown(link_type)
        || matches!(
            link_type,
            LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
        )
}
