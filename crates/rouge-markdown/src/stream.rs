use crate::document::{
    Definitions, FenceLine, LineIndex, MarkdownBlock, MarkdownBlockKind, MarkdownCodeBlock,
    MarkdownDocument, MarkdownOptions, MarkdownSourceStyle, MarkdownTarget, MarkdownTargetId,
    SyntaxCell, TailDefinition, assign_targets, fence_line, fence_marker, fenced_content_bounds,
    parse_from, push_code_line_targets, source_range,
};
use rouge_syntax::{SourceSyntax, SyntaxStream};
use std::{
    mem,
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownParseStats {
    pub parsed_bytes: usize,
    pub scanned_bytes: usize,
    pub source_bytes_copied: usize,
    pub parses: u64,
    pub unsettled_blocks: u64,
    pub highlighted_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownStreamUpdate {
    pub revision: u64,
    pub changed_source: Range<usize>,
    pub changed_blocks: Range<usize>,
    pub reset: bool,
    pub finished: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownStreamChanges {
    pub first_block: usize,
    pub first_line: usize,
    pub replaced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MarkdownStreamIdentity(u64);

impl Default for MarkdownStreamIdentity {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, Default)]
struct Settled {
    blocks: usize,
    items: usize,
    item_brackets: bool,
    targets: usize,
    styles: usize,
    source_end: usize,
    brackets: Vec<bool>,
    definitions: Vec<TailDefinition>,
    lookup: Definitions,
}

#[derive(Debug, Clone)]
struct OpenFence {
    block: usize,
    marker: u8,
    count: usize,
    start: usize,
    content_start: usize,
    line_start: usize,
    complete: usize,
    partial: FenceLine,
    syntax: Option<CodeSyntax>,
}

impl OpenFence {
    fn line(&self, bytes: &[u8]) -> FenceLine {
        fence_line(bytes, self.marker, self.count)
    }

    fn publish(
        &mut self,
        code: &mut MarkdownCodeBlock,
        finished: bool,
        stats: &mut MarkdownParseStats,
    ) -> Option<usize> {
        let definite = if finished {
            code.lines.len()
        } else {
            let complete = self.complete.min(code.lines.len());
            complete
                + usize::from(self.partial == FenceLine::Content && code.lines.len() > complete)
        };
        let changed = self
            .syntax
            .as_mut()
            .and_then(|syntax| syntax.feed(code, definite, &mut stats.highlighted_bytes));
        if changed.is_none() {
            self.syntax = None;
        }
        code.syntax = match &self.syntax {
            Some(syntax) if !finished => {
                SyntaxCell::from(SourceSyntax::new(Arc::clone(syntax.stream.highlights())))
            }
            Some(syntax) if syntax.holds(code) => SyntaxCell::from(syntax.stream.syntax()),
            _ => SyntaxCell::default(),
        };
        match (changed, &self.syntax) {
            (Some(line), Some(syntax)) => {
                (line < syntax.stream.highlights().line_count()).then_some(line)
            }
            _ => Some(0),
        }
    }
}

#[derive(Debug, Clone)]
struct CodeSyntax {
    stream: SyntaxStream,
}

impl CodeSyntax {
    fn new(code: &MarkdownCodeBlock) -> Self {
        Self {
            stream: SyntaxStream::new(code.language().unwrap_or_default()),
        }
    }

    fn feed(
        &mut self,
        code: &MarkdownCodeBlock,
        definite: usize,
        highlighted: &mut usize,
    ) -> Option<usize> {
        let end = code.definite_end(definite);
        let fed = self.stream.source().len();
        if end < fed || !code.text.is_char_boundary(fed) {
            *self = Self::new(code);
        }
        let fed = self.stream.source().len();
        let parsed = self.stream.work_stats().parser_input_bytes;
        let changed = self.stream.append(&code.text[fed..end]);
        *highlighted += self.stream.work_stats().parser_input_bytes - parsed;
        changed.ok().map(|update| update.changed_lines.start)
    }

    fn holds(&self, code: &MarkdownCodeBlock) -> bool {
        self.stream.source() == code.text
    }
}

#[derive(Debug, Default)]
pub struct MarkdownStream {
    document: MarkdownDocument,
    options: MarkdownOptions,
    index: LineIndex,
    settled: Settled,
    definitions: Definitions,
    tail_definitions: Vec<TailDefinition>,
    fence: Option<OpenFence>,
    stats: MarkdownParseStats,
    revision: u64,
    source_revision: u64,
    identity: MarkdownStreamIdentity,
    finished: bool,
    changes: Vec<(u64, (usize, usize))>,
    replaced_at: u64,
}

impl Clone for MarkdownStream {
    fn clone(&self) -> Self {
        Self {
            document: self.document.clone(),
            options: self.options,
            index: self.index.clone(),
            settled: self.settled.clone(),
            definitions: self.definitions.clone(),
            tail_definitions: self.tail_definitions.clone(),
            fence: self.fence.clone(),
            stats: self.stats,
            revision: self.revision,
            source_revision: self.source_revision,
            identity: MarkdownStreamIdentity::default(),
            finished: self.finished,
            changes: self.changes.clone(),
            replaced_at: self.replaced_at,
        }
    }
}

impl MarkdownStream {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_options(options: MarkdownOptions) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }

    pub fn push(&mut self, chunk: &str) -> MarkdownStreamUpdate {
        if chunk.is_empty() && !self.finished {
            return self.unchanged();
        }
        let start = self.source().len();
        let reset = mem::take(&mut self.finished);
        let mut first = (self.block_count(), 0);
        if !chunk.is_empty() {
            first = self.append(chunk);
            self.source_revision = self.source_revision.wrapping_add(1);
        }
        self.revision = self.revision.wrapping_add(1);
        self.record(first);
        self.update(start..self.source().len(), first.0, reset)
    }

    pub fn replace(&mut self, source: impl Into<String>) -> MarkdownStreamUpdate {
        let source = source.into();
        *self = Self {
            stats: self.stats,
            revision: self.revision,
            source_revision: self.source_revision,
            identity: self.identity,
            ..Self::with_options(self.options)
        };
        self.append(&source);
        self.source_revision = self.source_revision.wrapping_add(1);
        self.finished = false;
        self.revision = self.revision.wrapping_add(1);
        self.changes.clear();
        self.replaced_at = self.revision;
        self.update(0..self.source().len(), 0, true)
    }

    pub fn finish(&mut self) -> MarkdownStreamUpdate {
        if self.finished {
            return self.unchanged();
        }
        self.finished = true;
        self.revision = self.revision.wrapping_add(1);
        let first = self.finish_fence().unwrap_or((self.block_count(), 0));
        self.record(first);
        let len = self.source().len();
        self.update(len..len, first.0, false)
    }

    #[must_use]
    pub fn changes_since(&self, revision: u64) -> MarkdownStreamChanges {
        if revision < self.replaced_at {
            return MarkdownStreamChanges {
                first_block: 0,
                first_line: 0,
                replaced: true,
            };
        }
        let position = self
            .changes
            .partition_point(|(changed_at, _)| *changed_at <= revision);
        let (first_block, first_line) = self
            .changes
            .get(position)
            .map_or((self.block_count(), 0), |(_, first)| *first);
        MarkdownStreamChanges {
            first_block,
            first_line,
            replaced: false,
        }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        self.document.source()
    }

    #[must_use]
    pub const fn document(&self) -> &MarkdownDocument {
        &self.document
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }

    #[must_use]
    pub const fn identity(&self) -> MarkdownStreamIdentity {
        self.identity
    }

    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    #[must_use]
    pub const fn settled_blocks(&self) -> usize {
        self.settled.blocks
    }

    #[must_use]
    pub fn open_code_block(&self) -> Option<usize> {
        self.fence.as_ref().map(|fence| fence.block)
    }

    #[must_use]
    pub const fn parse_stats(&self) -> MarkdownParseStats {
        self.stats
    }

    fn block_count(&self) -> usize {
        self.document.blocks().len()
    }

    fn record(&mut self, first: (usize, usize)) {
        if first.0 >= self.block_count() {
            return;
        }
        while self
            .changes
            .last()
            .is_some_and(|(_, change)| *change >= first)
        {
            self.changes.pop();
        }
        self.changes.push((self.revision, first));
    }

    fn update(
        &self,
        changed_source: Range<usize>,
        first_block: usize,
        reset: bool,
    ) -> MarkdownStreamUpdate {
        MarkdownStreamUpdate {
            revision: self.revision,
            changed_source,
            changed_blocks: first_block.min(self.block_count())..self.block_count(),
            reset,
            finished: self.finished,
        }
    }

    fn unchanged(&self) -> MarkdownStreamUpdate {
        let len = self.source().len();
        self.update(len..len, self.block_count(), false)
    }

    fn finish_fence(&mut self) -> Option<(usize, usize)> {
        let fence = self.fence.as_mut()?;
        let MarkdownBlockKind::CodeBlock(code) = &mut self.document.blocks[fence.block].kind else {
            return None;
        };
        let line = fence.publish(Arc::make_mut(code), true, &mut self.stats)?;
        Some((fence.block, line))
    }

    fn append(&mut self, chunk: &str) -> (usize, usize) {
        let start = self.document.source.len();
        self.document.source.push_str(chunk);
        self.stats.source_bytes_copied += chunk.len();
        self.stats.scanned_bytes += chunk.len();
        self.index.extend(&self.document.source, start);
        if self.fence.is_some() {
            let block = self.fence.as_ref().map_or(0, |fence| fence.block);
            let (resume, line) = self.extend_fence(start);
            if let Some(resume) = resume {
                self.settle_to(block + 1, resume);
                self.fence = None;
                if resume < self.document.source.len() {
                    self.parse_tail();
                }
            }
            return (block, line);
        }
        (self.parse_tail(), 0)
    }

    fn parse_tail(&mut self) -> usize {
        let mut first_changed = self.settled.blocks;
        loop {
            first_changed = first_changed.min(self.settled.blocks);
            let parsed = parse_from(
                &self.document.source,
                &self.index,
                self.settled.source_end,
                &self.settled.lookup,
                self.options,
            );
            self.stats.parsed_bytes += parsed.parsed_bytes;
            self.stats.parses += 1;
            if !self.install(parsed.blocks, parsed.styles) {
                self.unsettle(self.settled.blocks);
                continue;
            }
            let mut definitions = self.settled.lookup.clone();
            for definition in &parsed.definitions {
                definitions.insert(definition.label.clone(), definition.definition.clone());
            }
            if definitions != self.definitions {
                self.definitions = definitions;
                let bracketed = self.settled.brackets.iter().position(|bracket| *bracket);
                let items = (self.settled.item_brackets).then_some(self.settled.blocks);
                if let Some(block) = bracketed.or(items) {
                    self.unsettle(block);
                    self.stats.unsettled_blocks += 1;
                    continue;
                }
            }
            self.tail_definitions = parsed.definitions;
            break;
        }
        self.settle_tail();
        self.detect_fence();
        first_changed
    }

    fn install(
        &mut self,
        mut blocks: Vec<MarkdownBlock>,
        styles: Vec<MarkdownSourceStyle>,
    ) -> bool {
        let settled = &self.settled;
        let document = &mut self.document;
        document.source_styles.truncate(settled.styles);
        document.source_styles.extend(styles);
        document.targets.truncate(settled.targets);
        assign_targets(&mut blocks, &mut document.targets);
        if settled.items == 0 {
            document.blocks.truncate(settled.blocks);
            document.blocks.extend(blocks);
            return true;
        }
        document.blocks.truncate(settled.blocks + 1);
        let mut tail = blocks.into_iter();
        let open = tail.next().and_then(|block| match block.kind {
            MarkdownBlockKind::List { items, .. } => Some((block.source, items)),
            _ => None,
        });
        let (Some((source, open)), Some(list)) = (open, document.blocks.last_mut()) else {
            return false;
        };
        let MarkdownBlockKind::List { items, .. } = &mut list.kind else {
            return false;
        };
        items.truncate(settled.items);
        items.extend(open);
        list.source = source_range(list.source.bytes.start..source.bytes.end, &self.index);
        document.blocks.extend(tail);
        true
    }

    fn unsettle(&mut self, block: usize) {
        let blocks = &self.document.blocks;
        let gap_start = block
            .checked_sub(1)
            .map_or(0, |previous| blocks[previous].source.bytes.end);
        let end = self.attached_start(blocks[block].source.bytes.start, gap_start);
        self.settled.blocks = block;
        self.settled.items = 0;
        self.settled.item_brackets = false;
        self.settled.source_end = end;
        self.settled.brackets.truncate(block);
        self.settled
            .definitions
            .retain(|definition| definition.span.end <= end);
        self.settled.lookup = Definitions::default();
        for definition in &self.settled.definitions {
            self.settled
                .lookup
                .insert(definition.label.clone(), definition.definition.clone());
        }
        self.sync_settled_counts();
        self.fence = None;
    }

    fn sync_settled_counts(&mut self) {
        let end = self.settled.source_end;
        let document = &self.document;
        self.settled.targets = document
            .targets
            .partition_point(|target| target.source.bytes.start < end);
        self.settled.styles = document
            .source_styles
            .partition_point(|style| style.source.bytes.start < end);
    }

    fn settle_tail(&mut self) {
        let blocks = &self.document.blocks;
        let mut settled = self.settled.blocks;
        let mut end = self.settled.source_end;
        while settled < blocks.len() {
            let Some(boundary) = self.settle_boundary(&blocks[settled], blocks.get(settled + 1))
            else {
                break;
            };
            end = boundary;
            settled += 1;
        }
        if settled > self.settled.blocks {
            self.settle_to(settled, end);
        }
        self.settle_items();
    }

    fn settle_items(&mut self) {
        let index = self.settled.blocks;
        if index + 1 != self.document.blocks.len() {
            return;
        }
        let MarkdownBlockKind::List { items, .. } = &self.document.blocks[index].kind else {
            return;
        };
        let Some(open) = items
            .len()
            .checked_sub(1)
            .filter(|open| *open > self.settled.items)
        else {
            return;
        };
        let start = items[open].source.bytes.start;
        let boundary = self.index.line_start_of(start);
        if !self.index.line_complete(start) || boundary < items[open - 1].source.bytes.end {
            return;
        }
        let source = &self.document.source;
        for item in &items[self.settled.items..open] {
            let bytes = &source[item.source.bytes.clone()];
            self.stats.scanned_bytes += bytes.len();
            self.settled.item_brackets |= bytes.contains('[');
        }
        self.settled.items = open;
        self.advance(boundary);
    }

    fn settle_to(&mut self, blocks: usize, end: usize) {
        let source = &self.document.source;
        for block in &self.document.blocks[self.settled.blocks..blocks] {
            let bracket = !matches!(block.kind, MarkdownBlockKind::CodeBlock(_)) && {
                let bytes = &source[block.source.bytes.clone()];
                self.stats.scanned_bytes += bytes.len();
                bytes.contains('[')
            };
            self.settled.brackets.push(bracket);
        }
        self.settled.blocks = blocks;
        self.settled.items = 0;
        self.settled.item_brackets = false;
        self.advance(end);
    }

    fn advance(&mut self, end: usize) {
        self.settled.source_end = end;
        self.sync_settled_counts();
        let mut remaining = Vec::new();
        for definition in mem::take(&mut self.tail_definitions) {
            if definition.span.end <= end {
                self.settled
                    .lookup
                    .insert(definition.label.clone(), definition.definition.clone());
                self.settled.definitions.push(definition);
            } else {
                remaining.push(definition);
            }
        }
        self.tail_definitions = remaining;
    }

    fn settle_boundary(
        &self,
        block: &MarkdownBlock,
        next: Option<&MarkdownBlock>,
    ) -> Option<usize> {
        let source = self.document.source.as_str();
        if let Some(next) = next {
            let start = next.source.bytes.start;
            return self
                .index
                .line_complete(start)
                .then(|| self.attached_start(start, block.source.bytes.end))
                .filter(|boundary| *boundary >= block.source.bytes.end);
        }
        let end = block.source.bytes.end;
        match &block.kind {
            MarkdownBlockKind::Heading { .. } | MarkdownBlockKind::Rule => {
                source[..end].ends_with('\n').then_some(end)
            }
            MarkdownBlockKind::Paragraph { .. }
            | MarkdownBlockKind::Table(_)
            | MarkdownBlockKind::BlockQuote { .. } => blank_line_at(source, end).then_some(end),
            MarkdownBlockKind::CodeBlock(code) => {
                (code.info.is_some() && end < source.len()).then(|| after_eol(source, end))
            }
            MarkdownBlockKind::List { .. } | MarkdownBlockKind::HtmlFallback { .. } => None,
        }
    }

    fn attached_start(&self, start: usize, gap_start: usize) -> usize {
        let source = self.document.source.as_str();
        let mut start = self.index.line_start_of(start);
        while start > gap_start {
            let previous = self.index.line_start_of(start - 1);
            if previous < gap_start || blank_line_at(source, previous) {
                break;
            }
            start = previous;
        }
        start
    }

    fn detect_fence(&mut self) {
        let source = self.document.source.as_str();
        let len = source.len();
        let index = self.document.blocks.len().checked_sub(1);
        let Some(index) = index.filter(|index| *index >= self.settled.blocks) else {
            return;
        };
        let block = &self.document.blocks[index];
        let MarkdownBlockKind::CodeBlock(code) = &block.kind else {
            return;
        };
        let start = block.source.bytes.start;
        if code.info.is_none() || block.source.bytes.end != len || !self.index.line_complete(start)
        {
            return;
        }
        let opening_end = source[start..]
            .find('\n')
            .map_or(len, |offset| start + offset + 1);
        let (marker, count) = fence_marker(&source[start..opening_end]);
        let line_start = self.index.line_start_of(len).max(opening_end);
        self.stats.scanned_bytes += len - line_start;
        let mut fence = OpenFence {
            block: index,
            marker,
            count,
            start,
            content_start: opening_end,
            line_start,
            complete: self.index.line_breaks_after(opening_end),
            partial: fence_line(&source.as_bytes()[line_start..], marker, count),
            syntax: Some(CodeSyntax::new(code)),
        };
        if let MarkdownBlockKind::CodeBlock(code) = &mut self.document.blocks[index].kind {
            fence.publish(Arc::make_mut(code), false, &mut self.stats);
        }
        self.fence = Some(fence);
    }

    fn extend_fence(&mut self, start: usize) -> (Option<usize>, usize) {
        let Some(mut fence) = self.fence.take() else {
            return (None, 0);
        };
        let MarkdownDocument {
            source,
            blocks,
            targets,
            ..
        } = &mut self.document;
        let MarkdownBlockKind::CodeBlock(code) = &mut blocks[fence.block].kind else {
            return (None, 0);
        };
        let code = Arc::make_mut(code);
        let bytes = source.as_bytes();
        let len = bytes.len();
        let was_closing = fence.partial == FenceLine::Closing;
        let complete = fence.complete;
        let mut close = None;
        let mut position = start;
        while let Some(offset) = bytes[position..]
            .iter()
            .position(|byte| matches!(byte, b'\n' | b'\r'))
        {
            let end = position + offset;
            if fence.line(&bytes[fence.line_start..end]) == FenceLine::Closing {
                close = Some((end, after_eol(source, end)));
                break;
            }
            position = end + 1;
            if bytes[end] == b'\n' {
                fence.line_start = position;
                fence.complete += 1;
            }
        }
        fence.partial = match close {
            Some(_) => FenceLine::Closing,
            None => fence.line(&bytes[fence.line_start..]),
        };
        let closing = fence.partial == FenceLine::Closing;
        let block_end = close.map_or(len, |(end, _)| end);
        let closing_start = closing.then_some(fence.line_start.max(fence.content_start));
        let content = fenced_content_bounds(fence.content_start, closing_start, block_end, source);
        let first_line = if was_closing || closing {
            complete.saturating_sub(1)
        } else {
            complete
        }
        .min(code.lines.len());
        let split = code.split_fenced_lines(first_line, content.clone(), source, &self.index);
        self.stats.scanned_bytes += len - start + split;
        code.content = source_range(content, &self.index);
        code.source = source_range(fence.start..block_end, &self.index);
        let block_source = code.source.clone();
        sync_code_targets(code, targets, first_line);
        let first_changed_line = fence
            .publish(code, close.is_some(), &mut self.stats)
            .map_or(first_line, |line| line.min(first_line));
        blocks[fence.block].source = block_source;
        let resume = close.map(|(_, resume)| resume);
        if resume.is_none() {
            self.fence = Some(fence);
        }
        (resume, first_changed_line)
    }
}

fn sync_code_targets(code: &mut MarkdownCodeBlock, targets: &mut Vec<MarkdownTarget>, from: usize) {
    let Some(base) = code.target_id.map(MarkdownTargetId::index) else {
        return;
    };
    if let Some(target) = targets.get_mut(base) {
        target.source = code.source.clone();
    }
    targets.truncate(base + 1 + from);
    push_code_line_targets(&mut code.lines[from..], targets);
}

fn blank_line_at(source: &str, position: usize) -> bool {
    let mut cursor = position;
    let bytes = source.as_bytes();
    while let Some(byte) = bytes.get(cursor) {
        match byte {
            b' ' | b'\t' | 0x0b | 0x0c => cursor += 1,
            b'\n' | b'\r' => return true,
            _ => return false,
        }
    }
    false
}

fn after_eol(source: &str, position: usize) -> usize {
    let bytes = source.as_bytes();
    match (bytes.get(position), bytes.get(position + 1)) {
        (Some(b'\r'), Some(b'\n')) => position + 2,
        (Some(b'\n' | b'\r'), _) => position + 1,
        _ => position,
    }
}
