use crate::{
    incremental::IncrementalDocument,
    text::{MarkdownText, openable},
};
use linkify::{LinkFinder, LinkKind};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, LinkType, Tag};
use rouge_syntax::{LanguageHint, SourceSyntax, analyze};
use serde::{Deserialize, Serialize};
use std::{
    cmp, fmt,
    ops::Range,
    slice,
    sync::{Arc, OnceLock},
};

/// A one-based, inclusive range of lines in the original Markdown source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct MarkdownLineRange {
    pub start: usize,
    pub end: usize,
}

/// A byte and line range into the original, unmodified source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub bytes: Range<usize>,
    pub lines: MarkdownLineRange,
}

/// An opaque identifier valid only for the lifetime of one parsed document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MarkdownTargetId(usize);

impl MarkdownTargetId {
    /// Returns the document-local ordinal of this target.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// The semantic kinds that can be selected and commented on.
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

/// A formatted inline Markdown node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownInline {
    Text(String),
    Code(String),
    Strong(Vec<MarkdownInline>),
    Emphasis(Vec<MarkdownInline>),
    Strikethrough(Vec<MarkdownInline>),
    Link {
        destination: String,
        title: Option<String>,
        content: Vec<MarkdownInline>,
    },
    SoftBreak,
    HardBreak,
    /// Images intentionally do not load assets. Their alt text is the fallback.
    ImageAlt(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownOptions {
    pub intra_doc_links: bool,
}

/// A commentable or structurally significant Markdown block.
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

/// The semantic kinds of blocks exposed to renderers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownBlockKind {
    Heading {
        level: u8,
        content: Vec<MarkdownInline>,
    },
    Paragraph {
        content: Vec<MarkdownInline>,
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
    /// Raw HTML is retained as escaped/plain fallback text, but is not a target.
    HtmlFallback {
        content: Vec<MarkdownInline>,
    },
    Rule,
}

/// One list item. The list container itself is not a review target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownListItem {
    pub depth: usize,
    pub source: SourceRange,
    pub content: Vec<MarkdownInline>,
    pub blocks: Vec<MarkdownBlock>,
    pub target_id: Option<MarkdownTargetId>,
}

/// A fenced or indented code block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownCodeBlock {
    pub(crate) language: Option<String>,
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
        self.language.as_deref()
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

    pub(crate) fn push_line(&mut self, (mut line, text): (MarkdownCodeLine, &str)) {
        if !self.lines.is_empty() {
            self.text.push('\n');
        }
        let start = self.text.len();
        self.text.push_str(text);
        line.text = start..self.text.len();
        self.lines.push(line);
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

    pub(crate) fn highlight_hint(&self) -> &str {
        self.info
            .as_deref()
            .or(self.language.as_deref())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn syntax(&self) -> &SourceSyntax {
        self.syntax
            .get(|| analyze(LanguageHint::InfoString(self.highlight_hint()), &self.text))
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
        for (line_index, line) in code.split('\n').enumerate() {
            let code_line = MarkdownCodeLine {
                index: from + line_index,
                source: source_range(offset..offset + line.len(), index),
                source_line: Some(first_line + line_index),
                text: 0..0,
                target_id: None,
            };
            self.push_line((code_line, line.strip_suffix('\r').unwrap_or(line)));
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

/// One content line inside a code block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownCodeLine {
    pub index: usize,
    pub source: SourceRange,
    pub source_line: Option<usize>,
    pub(crate) text: Range<usize>,
    pub target_id: Option<MarkdownTargetId>,
}

/// A table and its rows. Cells are structural and are not independent targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTable {
    pub alignments: Vec<MarkdownTableAlignment>,
    pub rows: Vec<MarkdownTableRow>,
}

/// Alignment requested by a Markdown table delimiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownTableAlignment {
    None,
    Left,
    Center,
    Right,
}

/// One table row, including the header row when one exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTableRow {
    pub header: bool,
    pub source: SourceRange,
    pub cells: Vec<MarkdownTableCell>,
    pub target_id: Option<MarkdownTargetId>,
}

/// One structural table cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTableCell {
    pub source: SourceRange,
    pub content: Vec<MarkdownInline>,
}

/// A heading in document outline order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownHeading {
    pub level: u8,
    pub title: String,
    pub source: SourceRange,
    pub target_id: MarkdownTargetId,
}

/// A selectable target in document order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTarget {
    pub id: MarkdownTargetId,
    pub kind: MarkdownTargetKind,
    pub source: SourceRange,
    pub display_label: String,
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

/// A parsed Markdown document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MarkdownDocument {
    pub(crate) source_path: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) source: String,
    pub(crate) blocks: Vec<MarkdownBlock>,
    pub(crate) outline: Vec<MarkdownHeading>,
    pub(crate) targets: Vec<MarkdownTarget>,
    pub(crate) source_styles: Vec<MarkdownSourceStyle>,
}

impl MarkdownDocument {
    /// Parses Markdown with the supported GFM extensions enabled.
    #[must_use]
    pub fn parse(source: impl Into<String>) -> Self {
        Self::parse_with(source, MarkdownOptions::default())
    }

    #[must_use]
    pub fn parse_with(source: impl Into<String>, options: MarkdownOptions) -> Self {
        IncrementalDocument::parse(source.into(), options)
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

    /// Returns the original source, including its original line endings.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the optional source path supplied by the host.
    #[must_use]
    pub fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }

    /// Returns the optional display title supplied by the host.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns the top-level semantic blocks.
    #[must_use]
    pub fn blocks(&self) -> &[MarkdownBlock] {
        &self.blocks
    }

    /// Returns headings in document order.
    #[must_use]
    pub fn outline(&self) -> &[MarkdownHeading] {
        &self.outline
    }

    /// Returns all commentable targets in document order.
    #[must_use]
    pub fn targets(&self) -> &[MarkdownTarget] {
        &self.targets
    }

    /// Finds a target by its document-local ID.
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

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub(crate) event: Option<Event<'static>>,
    pub(crate) range: Range<usize>,
    pub(crate) children: Vec<Node>,
}

const fn is_label_link(link_type: LinkType) -> bool {
    matches!(
        link_type,
        LinkType::ReferenceUnknown | LinkType::CollapsedUnknown | LinkType::ShortcutUnknown
    )
}

pub(crate) fn event_tree(events: &[(Event<'static>, Range<usize>)]) -> Vec<Node> {
    let mut stack = vec![Node {
        event: None,
        range: 0..0,
        children: Vec::new(),
    }];
    for (event, range) in events {
        match event {
            Event::Start(_) => {
                stack.push(Node {
                    event: Some(event.clone()),
                    range: range.clone(),
                    children: Vec::new(),
                });
            }
            Event::End(_) => {
                if stack.len() > 1
                    && let Some(mut node) = stack.pop()
                {
                    node.range.end = cmp::max(node.range.end, range.end);
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    }
                }
            }
            _ => {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(Node {
                        event: Some(event.clone()),
                        range: range.clone(),
                        children: Vec::new(),
                    });
                }
            }
        }
    }
    // pulldown-cmark promises balanced events. Recovering here still makes the
    // model useful if a future parser extension violates that promise.
    while stack.len() > 1 {
        if let Some(node) = stack.pop()
            && let Some(parent) = stack.last_mut()
        {
            parent.children.push(node);
        }
    }
    stack.pop().map_or_else(Vec::new, |root| root.children)
}

pub(crate) fn collect_source_styles(
    nodes: &[Node],
    index: &LineIndex,
    output: &mut Vec<MarkdownSourceStyle>,
) {
    for node in nodes {
        let role = match node.event.as_ref() {
            Some(Event::Start(Tag::Heading { .. })) => Some(MarkdownSourceRole::Heading),
            Some(Event::Start(Tag::Link { .. })) => Some(MarkdownSourceRole::Link),
            Some(Event::Start(Tag::BlockQuote(_))) => Some(MarkdownSourceRole::Quote),
            Some(Event::Start(Tag::Strong)) => Some(MarkdownSourceRole::Strong),
            Some(Event::Start(Tag::Emphasis)) => Some(MarkdownSourceRole::Emphasis),
            Some(Event::Start(Tag::Strikethrough)) => Some(MarkdownSourceRole::Strikethrough),
            Some(Event::Code(_)) => Some(MarkdownSourceRole::Code),
            _ => None,
        };
        if let Some(role) = role {
            output.push(MarkdownSourceStyle {
                source: source_range(node.range.clone(), index),
                role,
            });
        }
        collect_source_styles(&node.children, index, output);
    }
}

pub(crate) fn source_range(range: Range<usize>, index: &LineIndex) -> SourceRange {
    SourceRange {
        lines: index.range(range.clone()),
        bytes: range,
    }
}

pub(crate) fn parse_block(
    node: &Node,
    depth: usize,
    source: &str,
    line_index: &LineIndex,
) -> Option<MarkdownBlock> {
    let event = node.event.as_ref()?;
    let kind = match event {
        Event::Start(Tag::Heading { level, .. }) => MarkdownBlockKind::Heading {
            level: heading_level(*level),
            content: inline_content(&node.children),
        },
        Event::Start(Tag::Paragraph) => MarkdownBlockKind::Paragraph {
            content: inline_content(&node.children),
        },
        Event::Start(Tag::BlockQuote(_)) => MarkdownBlockKind::BlockQuote {
            blocks: node
                .children
                .iter()
                .filter_map(|child| parse_block(child, depth, source, line_index))
                .collect(),
        },
        Event::Start(Tag::List(start)) => MarkdownBlockKind::List {
            ordered: start.is_some(),
            start: *start,
            items: node
                .children
                .iter()
                .filter_map(|child| parse_item(child, depth, source, line_index))
                .collect(),
        },
        Event::Start(Tag::CodeBlock(kind)) => {
            MarkdownBlockKind::CodeBlock(Arc::new(parse_code_block(node, kind, source, line_index)))
        }
        Event::Start(Tag::Table(alignments)) => {
            MarkdownBlockKind::Table(parse_table(node, alignments, line_index))
        }
        Event::Rule => MarkdownBlockKind::Rule,
        Event::Start(Tag::HtmlBlock) => MarkdownBlockKind::HtmlFallback {
            content: html_fallback(&node.children),
        },
        _ => return None,
    };
    Some(MarkdownBlock {
        source: source_range(node.range.clone(), line_index),
        kind,
        target_id: None,
    })
}

fn parse_item(
    node: &Node,
    depth: usize,
    source: &str,
    line_index: &LineIndex,
) -> Option<MarkdownListItem> {
    if !matches!(node.event, Some(Event::Start(Tag::Item))) {
        return None;
    }
    let mut content = Vec::new();
    let mut blocks = Vec::new();
    let mut took_paragraph = false;
    for child in &node.children {
        if !took_paragraph && matches!(child.event, Some(Event::Start(Tag::Paragraph))) {
            content = inline_children(&child.children);
            took_paragraph = true;
        } else if let Some(inline) = inline_node(child) {
            // Tight list items may contain inline events directly rather than
            // a paragraph wrapper. Preserve those events as the item's content.
            content.push(inline);
        } else if let Some(block) = parse_block(child, depth + 1, source, line_index) {
            blocks.push(block);
        }
    }
    Some(MarkdownListItem {
        depth,
        source: source_range(node.range.clone(), line_index),
        content: autolink(content),
        blocks,
        target_id: None,
    })
}

fn parse_code_block(
    node: &Node,
    kind: &CodeBlockKind<'static>,
    source: &str,
    line_index: &LineIndex,
) -> MarkdownCodeBlock {
    let (info, language) = match kind {
        CodeBlockKind::Indented => (None, None),
        CodeBlockKind::Fenced(info) => {
            let info = info.to_string();
            let language = info.split_whitespace().next().map(str::to_owned);
            (Some(info), language)
        }
    };
    let block_source = source_range(node.range.clone(), line_index);
    let mut block = MarkdownCodeBlock {
        language,
        info,
        content: block_source.clone(),
        source: block_source,
        text: String::new(),
        lines: Vec::new(),
        target_id: None,
        syntax: SyntaxCell::default(),
    };
    if matches!(kind, CodeBlockKind::Fenced(_)) {
        let content = fenced_content_range(node.range.clone(), source);
        block.content = source_range(content.clone(), line_index);
        block.split_fenced_lines(0, content, source, line_index);
        return block;
    }
    let text = node
        .children
        .iter()
        .filter_map(|child| match child.event.as_ref()? {
            Event::Text(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect::<String>();
    let code = text.strip_suffix('\n').unwrap_or(&text);
    let first_source_line = line_index.line_for_offset(node.range.start);
    if !code.is_empty() {
        for (index, line) in code.split('\n').enumerate() {
            let source_line = first_source_line + index;
            block.push_line((
                MarkdownCodeLine {
                    index,
                    source: line_index.source_line_range(source_line),
                    source_line: Some(source_line),
                    text: 0..0,
                    target_id: None,
                },
                line.strip_suffix('\r').unwrap_or(line),
            ));
        }
    }
    block
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

fn parse_table(node: &Node, alignments: &[Alignment], line_index: &LineIndex) -> MarkdownTable {
    let rows = node
        .children
        .iter()
        .filter_map(|section| {
            let header = matches!(section.event, Some(Event::Start(Tag::TableHead)));
            if !matches!(section.event, Some(Event::Start(Tag::TableHead)))
                && !matches!(section.event, Some(Event::Start(Tag::TableRow)))
            {
                return None;
            }
            table_row(section, header, line_index)
        })
        .collect();
    MarkdownTable {
        alignments: alignments.iter().copied().map(table_alignment).collect(),
        rows,
    }
}

fn table_row(node: &Node, header: bool, line_index: &LineIndex) -> Option<MarkdownTableRow> {
    if !matches!(
        node.event,
        Some(Event::Start(Tag::TableRow | Tag::TableHead))
    ) {
        return None;
    }
    let cells = node
        .children
        .iter()
        .filter_map(|cell| {
            if !matches!(cell.event, Some(Event::Start(Tag::TableCell))) {
                return None;
            }
            Some(MarkdownTableCell {
                source: source_range(cell.range.clone(), line_index),
                content: inline_content(&cell.children),
            })
        })
        .collect();
    Some(MarkdownTableRow {
        header,
        source: source_range(node.range.clone(), line_index),
        cells,
        target_id: None,
    })
}

fn table_alignment(alignment: Alignment) -> MarkdownTableAlignment {
    match alignment {
        Alignment::None => MarkdownTableAlignment::None,
        Alignment::Left => MarkdownTableAlignment::Left,
        Alignment::Center => MarkdownTableAlignment::Center,
        Alignment::Right => MarkdownTableAlignment::Right,
    }
}

fn inline_children(children: &[Node]) -> Vec<MarkdownInline> {
    let mut inlines = Vec::new();
    for child in children {
        match &child.event {
            Some(Event::Start(Tag::Link { link_type, .. })) if is_label_link(*link_type) => {
                inlines.extend(inline_children(&child.children));
            }
            _ => inlines.extend(inline_node(child)),
        }
    }
    inlines
}

fn inline_content(children: &[Node]) -> Vec<MarkdownInline> {
    autolink(inline_children(children))
}

fn autolink(inlines: Vec<MarkdownInline>) -> Vec<MarkdownInline> {
    let mut merged: Vec<MarkdownInline> = Vec::with_capacity(inlines.len());
    for inline in inlines {
        match (inline, merged.last_mut()) {
            (MarkdownInline::Text(text), Some(MarkdownInline::Text(previous))) => {
                previous.push_str(&text);
            }
            (MarkdownInline::Strong(children), _) => {
                merged.push(MarkdownInline::Strong(autolink(children)));
            }
            (MarkdownInline::Emphasis(children), _) => {
                merged.push(MarkdownInline::Emphasis(autolink(children)));
            }
            (MarkdownInline::Strikethrough(children), _) => {
                merged.push(MarkdownInline::Strikethrough(autolink(children)));
            }
            (inline, _) => merged.push(inline),
        }
    }
    merged.into_iter().flat_map(split_links).collect()
}

fn split_links(inline: MarkdownInline) -> Vec<MarkdownInline> {
    let MarkdownInline::Text(text) = inline else {
        return vec![inline];
    };
    let mut output = Vec::new();
    let mut rest = 0;
    for link in LinkFinder::new().links(&text) {
        let destination = match link.kind() {
            LinkKind::Url if openable(link.as_str()) => link.as_str().to_owned(),
            LinkKind::Email => format!("mailto:{}", link.as_str()),
            _ => continue,
        };
        if link.start() > rest {
            output.push(MarkdownInline::Text(text[rest..link.start()].to_owned()));
        }
        let content = vec![MarkdownInline::Text(link.as_str().to_owned())];
        output.push(MarkdownInline::Link {
            destination,
            title: None,
            content,
        });
        rest = link.end();
    }
    if output.is_empty() {
        return vec![MarkdownInline::Text(text)];
    }
    if rest < text.len() {
        output.push(MarkdownInline::Text(text[rest..].to_owned()));
    }
    output
}

fn inline_node(node: &Node) -> Option<MarkdownInline> {
    let event = node.event.as_ref()?;
    Some(match event {
        Event::Text(text) => MarkdownInline::Text(text.to_string()),
        Event::Code(text) => MarkdownInline::Code(text.to_string()),
        Event::Start(Tag::Strong) => MarkdownInline::Strong(inline_children(&node.children)),
        Event::Start(Tag::Emphasis) => MarkdownInline::Emphasis(inline_children(&node.children)),
        Event::Start(Tag::Strikethrough) => {
            MarkdownInline::Strikethrough(inline_children(&node.children))
        }
        Event::Start(Tag::Link {
            dest_url, title, ..
        }) => MarkdownInline::Link {
            destination: dest_url.to_string(),
            title: (!title.is_empty()).then(|| title.to_string()),
            content: inline_children(&node.children),
        },
        Event::Start(Tag::Image { .. }) => {
            MarkdownInline::ImageAlt(plain_text(&inline_children(&node.children)))
        }
        Event::SoftBreak => MarkdownInline::SoftBreak,
        Event::HardBreak => MarkdownInline::HardBreak,
        Event::InlineHtml(html) | Event::Html(html) => MarkdownInline::Text(html.to_string()),
        Event::TaskListMarker(checked) => MarkdownInline::Text(if *checked {
            "☑".to_owned()
        } else {
            "☐".to_owned()
        }),
        // Math, footnotes, and metadata are not enabled by this parser. Keep a
        // useful textual fallback if a future option starts producing them.
        Event::InlineMath(text) | Event::DisplayMath(text) | Event::FootnoteReference(text) => {
            MarkdownInline::Text(text.to_string())
        }
        Event::Start(_) | Event::End(_) | Event::Rule => return None,
    })
}

fn html_fallback(children: &[Node]) -> Vec<MarkdownInline> {
    children
        .iter()
        .filter_map(|child| match child.event.as_ref()? {
            Event::Html(text) | Event::InlineHtml(text) => {
                Some(MarkdownInline::Text(text.to_string()))
            }
            _ => None,
        })
        .collect()
}

pub(crate) fn assign_targets(
    blocks: &mut [MarkdownBlock],
    targets: &mut Vec<MarkdownTarget>,
    outline: &mut Vec<MarkdownHeading>,
) {
    for block in blocks {
        let source = &block.source;
        block.target_id = match &mut block.kind {
            MarkdownBlockKind::Heading { level, content } => {
                let title = plain_text(content);
                let kind = MarkdownTargetKind::Heading;
                let id = push_target(kind, source.clone(), title.clone(), targets);
                outline.push(MarkdownHeading {
                    level: *level,
                    title,
                    source: source.clone(),
                    target_id: id,
                });
                Some(id)
            }
            MarkdownBlockKind::Paragraph { content } => {
                let label = plain_text(content);
                let kind = MarkdownTargetKind::Paragraph;
                Some(push_target(kind, source.clone(), label, targets))
            }
            MarkdownBlockKind::List { items, .. } => {
                assign_item_targets(items, targets);
                None
            }
            MarkdownBlockKind::BlockQuote { blocks } => {
                let label = block_text(blocks);
                let kind = MarkdownTargetKind::BlockQuote;
                Some(push_target(kind, source.clone(), label, targets))
            }
            MarkdownBlockKind::CodeBlock(code) => {
                let code = Arc::make_mut(code);
                let label = "Code block".to_owned();
                let kind = MarkdownTargetKind::CodeBlock;
                let id = push_target(kind, code.source.clone(), label, targets);
                code.target_id = Some(id);
                push_code_line_targets(&mut code.lines, targets);
                Some(id)
            }
            MarkdownBlockKind::Table(table) => {
                for row in &mut table.rows {
                    let kind = MarkdownTargetKind::TableRow;
                    let id = push_target(kind, row.source.clone(), row_text(row), targets);
                    row.target_id = Some(id);
                }
                None
            }
            MarkdownBlockKind::HtmlFallback { .. } | MarkdownBlockKind::Rule => None,
        };
    }
}

fn assign_item_targets(items: &mut [MarkdownListItem], targets: &mut Vec<MarkdownTarget>) {
    for item in items {
        let label = plain_text(&item.content);
        let kind = MarkdownTargetKind::ListItem;
        item.target_id = Some(push_target(kind, item.source.clone(), label, targets));
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
        let label = format!("Code line {}", line.index + 1);
        let kind = MarkdownTargetKind::CodeLine;
        line.target_id = Some(push_target(kind, line.source.clone(), label, targets));
    }
}

fn push_target(
    kind: MarkdownTargetKind,
    source: SourceRange,
    display_label: String,
    targets: &mut Vec<MarkdownTarget>,
) -> MarkdownTargetId {
    let id = MarkdownTargetId(targets.len());
    targets.push(MarkdownTarget {
        id,
        kind,
        source,
        display_label,
    });
    id
}

fn block_text(blocks: &[MarkdownBlock]) -> String {
    blocks
        .iter()
        .map(|block| match &block.kind {
            MarkdownBlockKind::Heading { content, .. }
            | MarkdownBlockKind::Paragraph { content }
            | MarkdownBlockKind::HtmlFallback { content } => plain_text(content),
            MarkdownBlockKind::List { items, .. } => items
                .iter()
                .map(|item| plain_text(&item.content))
                .collect::<Vec<_>>()
                .join(" "),
            MarkdownBlockKind::BlockQuote { blocks } => block_text(blocks),
            MarkdownBlockKind::CodeBlock(code) => code.text.clone(),
            MarkdownBlockKind::Table(table) => table
                .rows
                .iter()
                .map(row_text)
                .collect::<Vec<_>>()
                .join(" "),
            MarkdownBlockKind::Rule => String::new(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn row_text(row: &MarkdownTableRow) -> String {
    row.cells
        .iter()
        .map(|cell| plain_text(&cell.content))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn plain_text(inlines: &[MarkdownInline]) -> String {
    MarkdownText::new(inlines).text
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
