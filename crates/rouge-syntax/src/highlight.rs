use crate::{HighlightId, HighlightSpan};
use crate::{
    SyntaxError,
    incremental::{AppendContext, IncrementalDocument, SyntaxWorkStats, grammar},
    language::{LanguageId, resolve_hint, shebang_id},
    spans::Flattener,
    symbols::SymbolTags,
};
use std::{
    fmt,
    ops::Range,
    sync::{Arc, OnceLock},
};
use tree_sitter::Tree;

pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_INJECTION_DEPTH: usize = 3;
const CHUNK_LINES: usize = 256;

#[derive(Debug, Clone, Default)]
pub struct SourceSyntax {
    pub highlights: Arc<DocumentHighlights>,
    symbols: Arc<Symbols>,
    work: SyntaxWorkStats,
}

impl SourceSyntax {
    #[must_use]
    pub fn new(highlights: Arc<DocumentHighlights>) -> Self {
        Self {
            highlights,
            symbols: Arc::default(),
            work: SyntaxWorkStats::default(),
        }
    }

    #[must_use]
    pub const fn work_stats(&self) -> SyntaxWorkStats {
        self.work
    }

    #[must_use]
    pub fn symbols(&self) -> &Arc<SymbolTags> {
        self.symbols.tags.get_or_init(|| {
            Arc::new(
                self.symbols
                    .source
                    .as_ref()
                    .and_then(|source| {
                        let grammar = grammar(source.language)?;
                        Some(grammar.tags()?.extract(&source.tree, &source.text))
                    })
                    .unwrap_or_default(),
            )
        })
    }
}

#[derive(Default)]
struct Symbols {
    tags: OnceLock<Arc<SymbolTags>>,
    source: Option<TaggedSource>,
}

struct TaggedSource {
    tree: Tree,
    language: LanguageId,
    text: Arc<str>,
}

impl fmt::Debug for Symbols {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Symbols")
            .field("tags", &self.tags.get())
            .finish_non_exhaustive()
    }
}

#[must_use]
pub fn analyze(language: &str, source: &str) -> SourceSyntax {
    let mut stream = SyntaxStream::new(language);
    let _rejected = stream.append(source);
    stream.syntax()
}

#[derive(Debug, Clone, Default)]
pub struct SyntaxStream {
    hint: Option<LanguageId>,
    source: String,
    revision: u64,
    highlights: Arc<DocumentHighlights>,
    symbols: OnceLock<Arc<Symbols>>,
    document: Option<IncrementalDocument>,
    language: Option<LanguageId>,
    language_settled: bool,
    line_starts: Vec<usize>,
    work: SyntaxWorkStats,
    flattener: Flattener,
}

#[derive(Debug, Clone)]
pub struct SyntaxStreamUpdate {
    pub base_revision: u64,
    pub revision: u64,
    pub changed_lines: Range<usize>,
    pub highlights: Arc<DocumentHighlights>,
}

impl SyntaxStream {
    #[must_use]
    pub fn new(language: &str) -> Self {
        Self {
            hint: resolve_hint(language),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn highlights(&self) -> &Arc<DocumentHighlights> {
        &self.highlights
    }

    #[must_use]
    pub const fn work_stats(&self) -> SyntaxWorkStats {
        self.work
    }

    #[must_use]
    pub fn syntax(&self) -> SourceSyntax {
        let symbols = self.symbols.get_or_init(|| {
            let source = self.document.as_ref().and_then(|document| {
                document.grammar().tags()?;
                Some(TaggedSource {
                    tree: document.tree()?.clone(),
                    language: self.language?,
                    text: Arc::from(self.source.as_str()),
                })
            });
            Arc::new(Symbols {
                tags: OnceLock::new(),
                source,
            })
        });
        SourceSyntax {
            highlights: Arc::clone(&self.highlights),
            symbols: Arc::clone(symbols),
            work: self.work,
        }
    }

    pub fn append(&mut self, appended: &str) -> Result<SyntaxStreamUpdate, SyntaxError> {
        let base_revision = self.revision;
        let attempted = self.source.len().saturating_add(appended.len());
        if attempted > MAX_SOURCE_BYTES {
            return Err(SyntaxError::InputLimit {
                limit: MAX_SOURCE_BYTES,
                attempted,
            });
        }
        if appended.is_empty() {
            let end = self.highlights.line_count();
            return Ok(SyntaxStreamUpdate {
                base_revision,
                revision: self.revision,
                changed_lines: end..end,
                highlights: Arc::clone(&self.highlights),
            });
        }
        let previous_len = self.source.len();
        let previous_lines = self.line_starts.len();
        self.symbols = OnceLock::new();
        self.source.push_str(appended);
        if self.line_starts.is_empty() {
            self.line_starts.push(0);
        }
        self.line_starts.extend(
            appended
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(previous_len + index + 1)),
        );
        let mut work = SyntaxWorkStats::default();
        let parsed = self
            .parse(previous_len, &mut work)
            .map(|start| self.project(start, &mut work));
        self.work = self.work + work;
        let first_changed = match parsed {
            Ok(first_changed) => first_changed,
            Err(error) => {
                self.source.truncate(previous_len);
                self.line_starts.truncate(previous_lines);
                self.document = None;
                self.language_settled = false;
                return Err(error);
            }
        };
        self.revision = self.revision.wrapping_add(1);
        Ok(SyntaxStreamUpdate {
            base_revision,
            revision: self.revision,
            changed_lines: first_changed..self.highlights.line_count(),
            highlights: Arc::clone(&self.highlights),
        })
    }

    fn parse(
        &mut self,
        previous_len: usize,
        work: &mut SyntaxWorkStats,
    ) -> Result<usize, SyntaxError> {
        let mut language_changed = false;
        if !self.language_settled {
            let resolved = self.hint.or_else(|| shebang_id(&self.source));
            self.language_settled = self.hint.is_some() || self.line_starts.len() > 1;
            language_changed = resolved != self.language;
            if language_changed {
                self.document = None;
                self.language = resolved;
            }
        }
        let Some(language) = self.language else {
            return Ok(if language_changed { 0 } else { previous_len });
        };
        let document = match &mut self.document {
            Some(document) => document,
            slot => slot.insert(IncrementalDocument::new(language).ok_or_else(|| {
                SyntaxError::MissingGrammar {
                    language: language.name().to_owned(),
                }
            })?),
        };
        document.append(
            &self.source,
            0,
            MAX_INJECTION_DEPTH,
            &mut AppendContext {
                stats: work,
                line_starts: &self.line_starts,
            },
        )
    }

    fn project(&mut self, start: usize, work: &mut SyntaxWorkStats) -> usize {
        let first_line = self
            .line_starts
            .partition_point(|&offset| offset <= start)
            .saturating_sub(1);
        let byte_start = self.line_starts.get(first_line).copied().unwrap_or(0);
        if let Some(document) = &self.document {
            document.spans_from(byte_start, &mut self.flattener.raw);
        }
        let spans = self.flattener.flatten(&self.source);
        let previous = &self.highlights;
        let mut lines = HighlightsBuilder::prefix(previous, first_line.min(previous.line_count()));
        let mut first_changed = lines.line_count();
        let mut first_span = 0;
        work.projected_bytes += self.source.len() - byte_start;
        work.reused_lines += lines.line_count();
        for (line, &from) in self.line_starts.iter().enumerate().skip(first_line) {
            if from == self.source.len() {
                break;
            }
            let mut to = self
                .line_starts
                .get(line + 1)
                .copied()
                .unwrap_or(self.source.len());
            if to > from && self.source.as_bytes()[to - 1] == b'\n' {
                to -= 1;
            }
            if to > from && self.source.as_bytes()[to - 1] == b'\r' {
                to -= 1;
            }
            while spans
                .get(first_span)
                .is_some_and(|span| span.range.end <= from)
            {
                first_span += 1;
            }
            let projected = spans[first_span..]
                .iter()
                .take_while(|span| span.range.start < to)
                .filter_map(|span| {
                    let start = span.range.start.max(from);
                    let end = span.range.end.min(to);
                    (start < end)
                        .then(|| (relative(start - from), relative(end - from), span.highlight))
                });
            work.projected_lines += 1;
            if lines.push_line(projected, previous.line(line)) {
                work.reused_lines += 1;
                if first_changed == line {
                    first_changed += 1;
                }
            }
        }
        self.highlights = Arc::new(lines.finish());
        first_changed
    }
}

fn relative(offset: usize) -> u32 {
    u32::try_from(offset).expect("source limit fits u32")
}

#[derive(Debug, Clone, Default)]
pub struct DocumentHighlights {
    chunks: Vec<Arc<HighlightChunk>>,
    lines: usize,
}

impl DocumentHighlights {
    #[must_use]
    pub fn line(&self, index: usize) -> Option<HighlightLine<'_>> {
        (index < self.lines).then(|| self.chunks[index / CHUNK_LINES].line(index % CHUNK_LINES))
    }

    #[must_use]
    pub const fn line_count(&self) -> usize {
        self.lines
    }

    pub fn lines(&self) -> impl ExactSizeIterator<Item = HighlightLine<'_>> {
        (0..self.lines).map(|index| self.chunks[index / CHUNK_LINES].line(index % CHUNK_LINES))
    }
}

#[derive(Debug, Clone, Default)]
struct HighlightChunk {
    line_ends: Vec<u32>,
    spans: Vec<(u32, u32, HighlightId)>,
}

impl HighlightChunk {
    fn line(&self, index: usize) -> HighlightLine<'_> {
        let from = index
            .checked_sub(1)
            .map_or(0, |previous| self.line_ends[previous] as usize);
        let to = self.line_ends[index] as usize;
        HighlightLine {
            spans: &self.spans[from..to],
        }
    }

    fn prefix(&self, lines: usize) -> Self {
        let spans = lines
            .checked_sub(1)
            .map_or(0, |last| self.line_ends[last] as usize);
        Self {
            line_ends: self.line_ends[..lines].to_vec(),
            spans: self.spans[..spans].to_vec(),
        }
    }
}

struct HighlightsBuilder {
    chunks: Vec<Arc<HighlightChunk>>,
    tail: HighlightChunk,
    lines: usize,
}

impl HighlightsBuilder {
    fn prefix(previous: &DocumentHighlights, lines: usize) -> Self {
        let full = lines / CHUNK_LINES;
        let partial = lines % CHUNK_LINES;
        let tail = if partial == 0 {
            HighlightChunk::default()
        } else {
            previous.chunks[full].prefix(partial)
        };
        Self {
            chunks: previous.chunks[..full].to_vec(),
            tail,
            lines,
        }
    }

    const fn line_count(&self) -> usize {
        self.lines
    }

    fn push_line(
        &mut self,
        spans: impl Iterator<Item = (u32, u32, HighlightId)>,
        previous: Option<HighlightLine<'_>>,
    ) -> bool {
        let tail = &mut self.tail;
        tail.spans.extend(spans);
        tail.line_ends
            .push(u32::try_from(tail.spans.len()).expect("chunk spans fit u32"));
        let unchanged = previous == Some(tail.line(tail.line_ends.len() - 1));
        self.lines += 1;
        if tail.line_ends.len() == CHUNK_LINES {
            self.chunks.push(Arc::new(std::mem::take(tail)));
        }
        unchanged
    }

    fn finish(mut self) -> DocumentHighlights {
        if !self.tail.line_ends.is_empty() {
            self.chunks.push(Arc::new(self.tail));
        }
        DocumentHighlights {
            chunks: self.chunks,
            lines: self.lines,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct HighlightLine<'a> {
    spans: &'a [(u32, u32, HighlightId)],
}

impl<'a> HighlightLine<'a> {
    #[must_use]
    pub const fn len(&self) -> usize {
        self.spans.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    #[must_use]
    pub fn clipped(self, len: usize) -> Self {
        let kept = self
            .spans
            .partition_point(|&(_, end, _)| end as usize <= len);
        Self {
            spans: &self.spans[..kept],
        }
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = HighlightSpan> + use<'a> {
        self.spans
            .iter()
            .map(|&(start, end, highlight)| HighlightSpan {
                range: start as usize..end as usize,
                highlight,
            })
    }
}

impl fmt::Debug for HighlightLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
