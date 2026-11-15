use crate::text::{MarkdownText, TextBuilder};
use pulldown_cmark::{
    Alignment, BrokenLink, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use rouge_syntax::{SourceSyntax, analyze};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt,
    ops::Range,
    slice,
    sync::{Arc, OnceLock},
};
use unicase::UniCase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct MarkdownLineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub bytes: Range<usize>,
    pub lines: MarkdownLineRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MarkdownTargetId(usize);

impl MarkdownTargetId {
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarkdownTargetKind {
    Heading,
    Paragraph,
    ListItem,
    BlockQuote,
    TableRow,
    CodeBlock,
    CodeLine,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownOptions {
    pub intra_doc_links: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownBlock {
    pub source: SourceRange,
    pub kind: MarkdownBlockKind,
    pub target_id: Option<MarkdownTargetId>,
}

impl MarkdownBlock {
    #[must_use]
    pub fn code_blocks(&self) -> Vec<&MarkdownCodeBlock> {
        let mut output = Vec::new();
        collect_code_blocks(slice::from_ref(self), &mut output);
        output
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownBlockKind {
    Heading {
        level: u8,
        content: Arc<MarkdownText>,
    },
    Paragraph {
        content: Arc<MarkdownText>,
    },
    List {
        ordered: bool,
        start: Option<u64>,
        items: Vec<MarkdownListItem>,
    },
    BlockQuote {
        blocks: Vec<MarkdownBlock>,
    },
    CodeBlock(Arc<MarkdownCodeBlock>),
    Table(MarkdownTable),
    HtmlFallback {
        content: Arc<MarkdownText>,
    },
    Rule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownListItem {
    pub depth: usize,
    pub source: SourceRange,
    pub content: Arc<MarkdownText>,
    pub blocks: Vec<MarkdownBlock>,
    pub target_id: Option<MarkdownTargetId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownCodeBlock {
    pub(crate) info: Option<String>,
    pub source: SourceRange,
    pub content: SourceRange,
    pub(crate) text: String,
    pub(crate) lines: Vec<MarkdownCodeLine>,
    pub target_id: Option<MarkdownTargetId>,
    #[serde(skip)]
    pub(crate) syntax: SyntaxCell,
}

impl MarkdownCodeBlock {
    #[must_use]
    pub fn language(&self) -> Option<&str> {
        self.info.as_deref()?.split_whitespace().next()
    }

    #[must_use]
    pub fn lines(&self) -> &[MarkdownCodeLine] {
        &self.lines
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn line_text(&self, line: &MarkdownCodeLine) -> &str {
        self.text.get(line.text.clone()).unwrap_or_default()
    }

    #[must_use]
    pub fn syntax(&self) -> &SourceSyntax {
        self.syntax
            .get(|| analyze(self.language().unwrap_or_default(), &self.text))
    }

    pub(crate) fn push_line(&mut self, source: SourceRange, text: &str) {
        if !self.lines.is_empty() {
            self.text.push('\n');
        }
        let start = self.text.len();
        self.text.push_str(text);
        self.lines.push(MarkdownCodeLine {
            source,
            text: start..self.text.len(),
            target_id: None,
        });
    }

    pub(crate) fn truncate_lines(&mut self, lines: usize) {
        self.lines.truncate(lines);
        self.text
            .truncate(self.lines.last().map_or(0, |line| line.text.end));
    }

    pub(crate) fn definite_end(&self, lines: usize) -> usize {
        lines
            .checked_sub(1)
            .map_or(0, |last| self.lines[last].text.end)
    }

    pub(crate) fn split_fenced_lines(
        &mut self,
        from: usize,
        content: Range<usize>,
        source: &str,
        index: &LineIndex,
    ) -> usize {
        self.truncate_lines(from);
        if content.is_empty() {
            return 0;
        }
        let first_line = index.line_for_offset(content.start) + from;
        let mut offset = index.line_start(first_line);
        let Some(code) = source.get(offset..content.end) else {
            return 0;
        };
        for line in code.split('\n') {
            self.push_line(
                source_range(offset..offset + line.len(), index),
                line.strip_suffix('\r').unwrap_or(line),
            );
            offset += line.len() + 1;
        }
        code.len()
    }
}

#[derive(Clone, Default)]
pub(crate) struct SyntaxCell(Arc<OnceLock<SourceSyntax>>);

impl SyntaxCell {
    fn get(&self, analyze: impl FnOnce() -> SourceSyntax) -> &SourceSyntax {
        self.0.get_or_init(analyze)
    }
}

impl From<SourceSyntax> for SyntaxCell {
    fn from(syntax: SourceSyntax) -> Self {
        Self(Arc::new(OnceLock::from(syntax)))
    }
}

impl PartialEq for SyntaxCell {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for SyntaxCell {}

impl fmt::Debug for SyntaxCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SyntaxCell")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownCodeLine {
    pub source: SourceRange,
    pub(crate) text: Range<usize>,
    pub target_id: Option<MarkdownTargetId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTable {
    pub alignments: Vec<MarkdownTableAlignment>,
    pub rows: Vec<MarkdownTableRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownTableAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTableRow {
    pub header: bool,
    pub source: SourceRange,
    pub cells: Vec<MarkdownTableCell>,
    pub target_id: Option<MarkdownTargetId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTableCell {
    pub source: SourceRange,
    pub content: Arc<MarkdownText>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTarget {
    pub id: MarkdownTargetId,
    pub kind: MarkdownTargetKind,
    pub source: SourceRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownSourceRole {
    Heading,
    Link,
    Quote,
    Code,
    Strong,
    Emphasis,
    Strikethrough,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownSourceStyle {
    pub source: SourceRange,
    pub role: MarkdownSourceRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MarkdownDocument {
    pub(crate) source_path: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) source: String,
    pub(crate) blocks: Vec<MarkdownBlock>,
    pub(crate) targets: Vec<MarkdownTarget>,
    pub(crate) source_styles: Vec<MarkdownSourceStyle>,
}

impl MarkdownDocument {
    #[must_use]
    pub fn parse(source: impl Into<String>) -> Self {
        Self::parse_with(source, MarkdownOptions::default())
    }

    #[must_use]
    pub fn parse_with(source: impl Into<String>, options: MarkdownOptions) -> Self {
        let source = source.into();
        let index = LineIndex::new(&source);
        let parsed = parse_from(&source, &index, 0, &Definitions::default(), options);
        let mut document = Self {
            source,
            source_styles: parsed.styles,
            blocks: parsed.blocks,
            ..Self::default()
        };
        assign_targets(&mut document.blocks, &mut document.targets);
        document
    }

    #[must_use]
    pub fn with_metadata(mut self, source_path: Option<String>, title: Option<String>) -> Self {
        self.source_path = source_path;
        self.title = title;
        self
    }

    #[must_use]
    pub fn source_styles(&self) -> &[MarkdownSourceStyle] {
        &self.source_styles
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }

    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    #[must_use]
    pub fn blocks(&self) -> &[MarkdownBlock] {
        &self.blocks
    }

    #[must_use]
    pub fn targets(&self) -> &[MarkdownTarget] {
        &self.targets
    }

    #[must_use]
    pub fn target(&self, id: MarkdownTargetId) -> Option<&MarkdownTarget> {
        self.targets.get(id.0)
    }
}

fn collect_code_blocks<'a>(blocks: &'a [MarkdownBlock], output: &mut Vec<&'a MarkdownCodeBlock>) {
    for block in blocks {
        match &block.kind {
            MarkdownBlockKind::CodeBlock(code) => output.push(code),
            MarkdownBlockKind::List { items, .. } => {
                for item in items {
                    collect_code_blocks(&item.blocks, output);
                }
            }
            MarkdownBlockKind::BlockQuote { blocks } => collect_code_blocks(blocks, output),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Definition {
    pub destination: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Definitions(HashMap<UniCase<String>, Definition>);

impl Definitions {
    pub(crate) fn get(&self, label: &str) -> Option<&Definition> {
        self.0.get(&label_key(label))
    }

    pub(crate) fn insert(&mut self, label: UniCase<String>, definition: Definition) {
        self.0.entry(label).or_insert(definition);
    }
}

pub(crate) fn label_key(label: &str) -> UniCase<String> {
    UniCase::new(label.split_ascii_whitespace().collect::<Vec<_>>().join(" "))
}

#[derive(Debug, Clone)]
pub(crate) struct TailDefinition {
    pub span: Range<usize>,
    pub label: UniCase<String>,
    pub definition: Definition,
}

pub(crate) struct TailParse {
    pub blocks: Vec<MarkdownBlock>,
    pub styles: Vec<MarkdownSourceStyle>,
    pub definitions: Vec<TailDefinition>,
    pub parsed_bytes: usize,
}

pub(crate) fn parse_from(
    source: &str,
    index: &LineIndex,
    start: usize,
    settled: &Definitions,
    options: MarkdownOptions,
) -> TailParse {
    let text = &source[start..];
    let callback = |link: BrokenLink<'_>| {
        (settled.get(&link.reference).is_some()
            || (options.intra_doc_links && is_intra_doc_link(text, &link)))
        .then_some((CowStr::Borrowed(""), CowStr::Borrowed("")))
    };
    let parser = Parser::new_with_broken_link_callback(
        text,
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH,
        Some(callback),
    );
    let mut definitions = parser
        .reference_definitions()
        .iter()
        .map(|(label, definition)| TailDefinition {
            span: definition.span.start + start..definition.span.end + start,
            label: label_key(label),
            definition: Definition {
                destination: definition.dest.to_string(),
                title: definition.title.as_ref().map(ToString::to_string),
            },
        })
        .collect::<Vec<_>>();
    definitions.sort_by_key(|definition| definition.span.start);
    let mut builder = Builder {
        source,
        index,
        settled,
        stack: Vec::new(),
        blocks: Vec::new(),
        styles: Vec::new(),
    };
    for (event, range) in parser.into_offset_iter() {
        builder.event(&event, range.start + start..range.end + start);
    }
    TailParse {
        blocks: builder.blocks,
        styles: builder.styles,
        definitions,
        parsed_bytes: text.len(),
    }
}

fn is_intra_doc_link(source: &str, link: &BrokenLink<'_>) -> bool {
    if link.link_type == pulldown_cmark::LinkType::Reference {
        return true;
    }
    let label = link.reference.as_ref();
    let follows_word = source[..link.span.start]
        .chars()
        .next_back()
        .is_some_and(|character| character.is_alphanumeric() || character == '_');
    let code = label.len() > 1 && label.starts_with('`') && label.ends_with('`');
    let path = label.chars().next().is_some_and(char::is_alphabetic)
        && label
            .chars()
            .all(|character| character.is_alphanumeric() || "_:.<>!()".contains(character));
    !follows_word && (code || path)
}

enum Sink {
    Paragraph,
    Heading(u8),
    Html,
    Cell,
}

enum Frame {
    Inline {
        sink: Sink,
        range: Range<usize>,
        text: TextBuilder,
    },
    ItemParagraph,
    Quote {
        range: Range<usize>,
        blocks: Vec<MarkdownBlock>,
    },
    List {
        range: Range<usize>,
        start: Option<u64>,
        depth: usize,
        items: Vec<MarkdownListItem>,
    },
    Item {
        range: Range<usize>,
        content: TextBuilder,
        took_paragraph: bool,
        blocks: Vec<MarkdownBlock>,
    },
    Code {
        range: Range<usize>,
        info: Option<String>,
        text: String,
    },
    Table {
        range: Range<usize>,
        alignments: Vec<MarkdownTableAlignment>,
        rows: Vec<MarkdownTableRow>,
    },
    Row {
        range: Range<usize>,
        header: bool,
        cells: Vec<MarkdownTableCell>,
    },
    Skip,
}

struct Builder<'a> {
    source: &'a str,
    index: &'a LineIndex,
    settled: &'a Definitions,
    stack: Vec<Frame>,
    blocks: Vec<MarkdownBlock>,
    styles: Vec<MarkdownSourceStyle>,
}

const fn is_inline(tag: TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Strong
            | TagEnd::Emphasis
            | TagEnd::Strikethrough
            | TagEnd::Link
            | TagEnd::Image
            | TagEnd::Superscript
            | TagEnd::Subscript
    )
}

impl Builder<'_> {
    fn event(&mut self, event: &Event<'_>, range: Range<usize>) {
        self.style(event, &range);
        if matches!(self.stack.last(), Some(Frame::Skip)) {
            match event {
                Event::Start(_) => self.stack.push(Frame::Skip),
                Event::End(_) => {
                    self.stack.pop();
                }
                _ => {}
            }
            return;
        }
        match event {
            Event::Start(tag) if is_inline(tag.to_end()) => {
                let settled = self.settled;
                if let Some(text) = self.sink() {
                    text.start(tag, |label| settled.get(label).cloned());
                } else {
                    self.stack.push(Frame::Skip);
                }
            }
            Event::End(tag) if is_inline(*tag) => {
                if let Some(text) = self.sink() {
                    text.end();
                }
            }
            Event::Start(tag) => self.start(tag, range),
            Event::End(_) => self.end(range),
            Event::Rule => {
                let block = self.block(range, MarkdownBlockKind::Rule);
                self.attach(block);
            }
            Event::Text(text) if matches!(self.stack.last(), Some(Frame::Code { .. })) => {
                if let Some(Frame::Code { text: code, .. }) = self.stack.last_mut() {
                    code.push_str(text);
                }
            }
            event => {
                if let Some(text) = self.sink() {
                    text.event(event);
                }
            }
        }
    }

    fn style(&mut self, event: &Event<'_>, range: &Range<usize>) {
        let role = match event {
            Event::Start(Tag::Heading { .. }) => MarkdownSourceRole::Heading,
            Event::Start(Tag::Link { .. }) => MarkdownSourceRole::Link,
            Event::Start(Tag::BlockQuote(_)) => MarkdownSourceRole::Quote,
            Event::Start(Tag::Strong) => MarkdownSourceRole::Strong,
            Event::Start(Tag::Emphasis) => MarkdownSourceRole::Emphasis,
            Event::Start(Tag::Strikethrough) => MarkdownSourceRole::Strikethrough,
            Event::Code(_) => MarkdownSourceRole::Code,
            _ => return,
        };
        self.styles.push(MarkdownSourceStyle {
            source: source_range(range.clone(), self.index),
            role,
        });
    }

    fn sink(&mut self) -> Option<&mut TextBuilder> {
        let paragraph = matches!(self.stack.last(), Some(Frame::ItemParagraph));
        let frames = self.stack.len();
        let frame = self.stack.get_mut(frames - 1 - usize::from(paragraph))?;
        match frame {
            Frame::Inline { text, .. } => Some(text),
            Frame::Item { content, .. } => Some(content),
            _ => None,
        }
    }

    fn start(&mut self, tag: &Tag<'_>, range: Range<usize>) {
        let parent = self.stack.last_mut();
        let frame = match (tag, parent) {
            (Tag::Paragraph, Some(Frame::Item { took_paragraph, .. })) if !*took_paragraph => {
                *took_paragraph = true;
                Frame::ItemParagraph
            }
            (Tag::Paragraph, _) => inline(Sink::Paragraph, range),
            (Tag::Heading { level, .. }, _) => inline(Sink::Heading(heading_level(*level)), range),
            (Tag::HtmlBlock, _) => Frame::Inline {
                sink: Sink::Html,
                range,
                text: TextBuilder::raw(),
            },
            (Tag::BlockQuote(_), _) => Frame::Quote {
                range,
                blocks: Vec::new(),
            },
            (Tag::List(start), _) => Frame::List {
                range,
                start: *start,
                depth: self
                    .stack
                    .iter()
                    .filter(|frame| matches!(frame, Frame::Item { .. }))
                    .count(),
                items: Vec::new(),
            },
            (Tag::Item, Some(Frame::List { .. })) => Frame::Item {
                range,
                content: TextBuilder::default(),
                took_paragraph: false,
                blocks: Vec::new(),
            },
            (Tag::CodeBlock(kind), _) => Frame::Code {
                range,
                info: match kind {
                    CodeBlockKind::Indented => None,
                    CodeBlockKind::Fenced(info) => Some(info.to_string()),
                },
                text: String::new(),
            },
            (Tag::Table(alignments), _) => Frame::Table {
                range,
                alignments: alignments.iter().copied().map(table_alignment).collect(),
                rows: Vec::new(),
            },
            (Tag::TableHead | Tag::TableRow, Some(Frame::Table { .. })) => Frame::Row {
                range,
                header: matches!(tag, Tag::TableHead),
                cells: Vec::new(),
            },
            (Tag::TableCell, Some(Frame::Row { .. })) => inline(Sink::Cell, range),
            _ => Frame::Skip,
        };
        self.stack.push(frame);
    }

    fn end(&mut self, end: Range<usize>) {
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let range = |range: Range<usize>| range.start..range.end.max(end.end);
        let kind = match frame {
            Frame::Skip | Frame::ItemParagraph => return,
            Frame::Inline {
                sink: Sink::Cell,
                range: cell,
                text,
            } => {
                if let Some(Frame::Row { cells, .. }) = self.stack.last_mut() {
                    cells.push(MarkdownTableCell {
                        source: source_range(range(cell), self.index),
                        content: Arc::new(text.finish()),
                    });
                }
                return;
            }
            Frame::Row {
                range: row,
                header,
                cells,
            } => {
                if let Some(Frame::Table { rows, .. }) = self.stack.last_mut() {
                    rows.push(MarkdownTableRow {
                        header,
                        source: source_range(range(row), self.index),
                        cells,
                        target_id: None,
                    });
                }
                return;
            }
            Frame::Item {
                range: item,
                content,
                blocks,
                ..
            } => {
                if let Some(Frame::List { items, depth, .. }) = self.stack.last_mut() {
                    items.push(MarkdownListItem {
                        depth: *depth,
                        source: source_range(range(item), self.index),
                        content: Arc::new(content.finish()),
                        blocks,
                        target_id: None,
                    });
                }
                return;
            }
            Frame::Inline {
                sink,
                range: block,
                text,
            } => {
                let content = Arc::new(text.finish());
                let kind = match sink {
                    Sink::Heading(level) => MarkdownBlockKind::Heading { level, content },
                    Sink::Html => MarkdownBlockKind::HtmlFallback { content },
                    Sink::Paragraph | Sink::Cell => MarkdownBlockKind::Paragraph { content },
                };
                (block, kind)
            }
            Frame::Quote {
                range: block,
                blocks,
            } => (block, MarkdownBlockKind::BlockQuote { blocks }),
            Frame::List {
                range: block,
                start,
                items,
                ..
            } => (
                block,
                MarkdownBlockKind::List {
                    ordered: start.is_some(),
                    start,
                    items,
                },
            ),
            Frame::Code {
                range: block,
                info,
                text,
            } => {
                let block = range(block);
                let code = self.code_block(block.clone(), info, &text);
                (block, MarkdownBlockKind::CodeBlock(Arc::new(code)))
            }
            Frame::Table {
                range: block,
                alignments,
                rows,
            } => (
                block,
                MarkdownBlockKind::Table(MarkdownTable { alignments, rows }),
            ),
        };
        let block = self.block(range(kind.0), kind.1);
        self.attach(block);
    }

    fn block(&self, range: Range<usize>, kind: MarkdownBlockKind) -> MarkdownBlock {
        MarkdownBlock {
            source: source_range(range, self.index),
            kind,
            target_id: None,
        }
    }

    fn attach(&mut self, block: MarkdownBlock) {
        match self.stack.last_mut() {
            None => self.blocks.push(block),
            Some(Frame::Quote { blocks, .. } | Frame::Item { blocks, .. }) => blocks.push(block),
            Some(_) => {}
        }
    }

    fn code_block(
        &self,
        range: Range<usize>,
        info: Option<String>,
        text: &str,
    ) -> MarkdownCodeBlock {
        let block_source = source_range(range.clone(), self.index);
        let fenced = info.is_some();
        let mut block = MarkdownCodeBlock {
            info,
            content: block_source.clone(),
            source: block_source,
            text: String::new(),
            lines: Vec::new(),
            target_id: None,
            syntax: SyntaxCell::default(),
        };
        if fenced {
            let content = fenced_content_range(range, self.source);
            block.content = source_range(content.clone(), self.index);
            block.split_fenced_lines(0, content, self.source, self.index);
            return block;
        }
        let code = text.strip_suffix('\n').unwrap_or(text);
        let first_source_line = self.index.line_for_offset(range.start);
        if !code.is_empty() {
            for (index, line) in code.split('\n').enumerate() {
                block.push_line(
                    self.index.source_line_range(first_source_line + index),
                    line.strip_suffix('\r').unwrap_or(line),
                );
            }
        }
        block
    }
}

fn inline(sink: Sink, range: Range<usize>) -> Frame {
    Frame::Inline {
        sink,
        range,
        text: TextBuilder::default(),
    }
}

pub(crate) fn source_range(range: Range<usize>, index: &LineIndex) -> SourceRange {
    SourceRange {
        lines: index.range(range.clone()),
        bytes: range,
    }
}

pub(crate) fn fenced_content_range(block: Range<usize>, source: &str) -> Range<usize> {
    let start = block.start.min(source.len());
    let end = block.end.min(source.len());
    let opening_end = source[start..end]
        .find('\n')
        .map_or(end, |offset| start + offset + 1);
    if opening_end >= end {
        return opening_end..opening_end;
    }
    let (marker, count) = fence_marker(&source[start..opening_end]);
    let last_line = source[opening_end..end]
        .rfind('\n')
        .map_or(opening_end, |offset| opening_end + offset + 1);
    let closing = fence_line(&source.as_bytes()[last_line..end], marker, count);
    let closing_start = (closing == FenceLine::Closing).then_some(last_line);
    fenced_content_bounds(opening_end, closing_start, end, source)
}

pub(crate) fn fenced_content_bounds(
    opening_end: usize,
    closing_start: Option<usize>,
    end: usize,
    source: &str,
) -> Range<usize> {
    let mut content_end = closing_start.unwrap_or(end);
    if closing_start.is_some() && source[..content_end].ends_with('\n') {
        content_end -= 1;
        if content_end > opening_end && source.as_bytes()[content_end - 1] == b'\r' {
            content_end -= 1;
        }
    }
    opening_end.min(content_end)..content_end
}

pub(crate) fn fence_marker(opening_line: &str) -> (u8, usize) {
    let trimmed = opening_line.trim_start_matches([' ', '\t']);
    let marker = trimmed.bytes().next().unwrap_or(b'`');
    let count = trimmed.bytes().take_while(|byte| *byte == marker).count();
    (marker, count)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FenceLine {
    Closing,
    Partial,
    Content,
}

pub(crate) fn fence_line(line: &[u8], marker: u8, count: usize) -> FenceLine {
    let mut columns = 0;
    let mut cursor = 0;
    while let Some(byte) = line.get(cursor) {
        match byte {
            b' ' => columns += 1,
            b'\t' => columns = (columns / 4 + 1) * 4,
            _ => break,
        }
        if columns >= 4 {
            return FenceLine::Content;
        }
        cursor += 1;
    }
    let run = line[cursor..]
        .iter()
        .take_while(|byte| **byte == marker)
        .count();
    cursor += run;
    if cursor == line.len() {
        return if run >= count {
            FenceLine::Closing
        } else {
            FenceLine::Partial
        };
    }
    if run < count {
        return FenceLine::Content;
    }
    while line.get(cursor) == Some(&b' ') {
        cursor += 1;
    }
    match line.get(cursor) {
        None | Some(b'\r') => FenceLine::Closing,
        Some(_) => FenceLine::Content,
    }
}

fn table_alignment(alignment: Alignment) -> MarkdownTableAlignment {
    match alignment {
        Alignment::None => MarkdownTableAlignment::None,
        Alignment::Left => MarkdownTableAlignment::Left,
        Alignment::Center => MarkdownTableAlignment::Center,
        Alignment::Right => MarkdownTableAlignment::Right,
    }
}

pub(crate) fn assign_targets(blocks: &mut [MarkdownBlock], targets: &mut Vec<MarkdownTarget>) {
    for block in blocks {
        let source = &block.source;
        block.target_id = match &mut block.kind {
            MarkdownBlockKind::Heading { .. } => {
                Some(push_target(MarkdownTargetKind::Heading, source, targets))
            }
            MarkdownBlockKind::Paragraph { .. } => {
                Some(push_target(MarkdownTargetKind::Paragraph, source, targets))
            }
            MarkdownBlockKind::List { items, .. } => {
                assign_item_targets(items, targets);
                None
            }
            MarkdownBlockKind::BlockQuote { .. } => {
                Some(push_target(MarkdownTargetKind::BlockQuote, source, targets))
            }
            MarkdownBlockKind::CodeBlock(code) => {
                let code = Arc::make_mut(code);
                let id = push_target(MarkdownTargetKind::CodeBlock, &code.source, targets);
                code.target_id = Some(id);
                push_code_line_targets(&mut code.lines, targets);
                Some(id)
            }
            MarkdownBlockKind::Table(table) => {
                for row in &mut table.rows {
                    row.target_id = Some(push_target(
                        MarkdownTargetKind::TableRow,
                        &row.source,
                        targets,
                    ));
                }
                None
            }
            MarkdownBlockKind::HtmlFallback { .. } | MarkdownBlockKind::Rule => None,
        };
    }
}

fn assign_item_targets(items: &mut [MarkdownListItem], targets: &mut Vec<MarkdownTarget>) {
    for item in items {
        item.target_id = Some(push_target(
            MarkdownTargetKind::ListItem,
            &item.source,
            targets,
        ));
        for block in &mut item.blocks {
            if let MarkdownBlockKind::List { items, .. } = &mut block.kind {
                assign_item_targets(items, targets);
            }
        }
    }
}

pub(crate) fn push_code_line_targets(
    lines: &mut [MarkdownCodeLine],
    targets: &mut Vec<MarkdownTarget>,
) {
    for line in lines {
        line.target_id = Some(push_target(
            MarkdownTargetKind::CodeLine,
            &line.source,
            targets,
        ));
    }
}

fn push_target(
    kind: MarkdownTargetKind,
    source: &SourceRange,
    targets: &mut Vec<MarkdownTarget>,
) -> MarkdownTargetId {
    let id = MarkdownTargetId(targets.len());
    targets.push(MarkdownTarget {
        id,
        kind,
        source: source.clone(),
    });
    id
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LineIndex {
    starts: Vec<usize>,
    content_ends: Vec<usize>,
    source_len: usize,
}

impl Default for LineIndex {
    fn default() -> Self {
        Self::new("")
    }
}

impl LineIndex {
    pub(crate) fn new(source: &str) -> Self {
        let mut index = Self {
            starts: vec![0],
            content_ends: vec![0],
            source_len: 0,
        };
        index.extend(source, 0);
        index
    }

    pub(crate) fn extend(&mut self, source: &str, from: usize) {
        self.content_ends.pop();
        let bytes = source.as_bytes();
        for (offset, byte) in bytes[from..].iter().enumerate() {
            let index = from + offset;
            if *byte == b'\n' {
                self.content_ends.push(
                    index.saturating_sub(usize::from(index > 0 && bytes[index - 1] == b'\r')),
                );
                self.starts.push(index + 1);
            }
        }
        self.content_ends.push(source.len());
        self.source_len = source.len();
    }

    pub(crate) fn line_start_of(&self, offset: usize) -> usize {
        let line = self.starts.partition_point(|start| *start <= offset);
        self.starts[line.saturating_sub(1)]
    }

    pub(crate) fn line_complete(&self, offset: usize) -> bool {
        self.starts.last().is_some_and(|start| *start > offset)
    }

    pub(crate) fn line_breaks_after(&self, offset: usize) -> usize {
        self.starts.len() - self.starts.partition_point(|start| *start <= offset)
    }

    pub(crate) fn line_start(&self, line: usize) -> usize {
        self.starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(self.source_len)
    }

    pub(crate) fn line_for_offset(&self, offset: usize) -> usize {
        let offset = offset.min(*self.starts.last().unwrap_or(&0));
        self.starts.partition_point(|start| *start <= offset)
    }

    fn range(&self, range: Range<usize>) -> MarkdownLineRange {
        let start = self.line_for_offset(range.start);
        let end_offset = range.end.saturating_sub(1).max(range.start);
        let end = self.line_for_offset(end_offset).max(start);
        MarkdownLineRange { start, end }
    }

    fn source_line_range(&self, line: usize) -> SourceRange {
        let start = self
            .starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        let end = self
            .content_ends
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(self.source_len)
            .max(start);
        SourceRange {
            bytes: start..end,
            lines: MarkdownLineRange {
                start: line,
                end: line,
            },
        }
    }
}
