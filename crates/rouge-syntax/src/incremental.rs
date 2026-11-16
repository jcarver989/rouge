use crate::HighlightId;
use crate::{
    SyntaxError,
    language::{LANGUAGES, Language, LanguageId, resolve_hint, shebang_id},
    spans::{Injection, Span},
    symbols::TagsGrammar,
};
use arborium_theme::tag_for_capture;
cfg_select! {
    target_arch = "wasm32" => {
        use std::rc::Rc as SharedGrammar;
    }
    _ => {
        use std::sync::Arc as SharedGrammar;
    }
}
use std::{borrow::Cow, collections::HashSet, fmt, ops::Add, sync::OnceLock};
use tree_sitter::{
    InputEdit, Language as TreeLanguage, Node, Parser, Point, Query, QueryCursor,
    StreamingIterator, Tree,
};

const PARSER_CHUNK_BYTES: usize = 16 << 10;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyntaxWorkStats {
    pub parser_input_bytes: usize,
    pub queried_bytes: usize,
    pub full_parses: usize,
    pub incremental_parses: usize,
    pub projected_bytes: usize,
    pub projected_lines: usize,
    pub reused_lines: usize,
    pub compared_nodes: usize,
}

impl SyntaxWorkStats {
    #[must_use]
    pub const fn parses(&self) -> usize {
        self.full_parses + self.incremental_parses
    }
}

impl Add for SyntaxWorkStats {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            parser_input_bytes: self.parser_input_bytes + other.parser_input_bytes,
            queried_bytes: self.queried_bytes + other.queried_bytes,
            full_parses: self.full_parses + other.full_parses,
            incremental_parses: self.incremental_parses + other.incremental_parses,
            projected_bytes: self.projected_bytes + other.projected_bytes,
            projected_lines: self.projected_lines + other.projected_lines,
            reused_lines: self.reused_lines + other.reused_lines,
            compared_nodes: self.compared_nodes + other.compared_nodes,
        }
    }
}

type Compiled = OnceLock<Option<SharedGrammar<Grammar>>>;

cfg_select! {
    target_arch = "wasm32" => {
        fn with_registry<T>(read: impl FnOnce(&[Compiled]) -> T) -> T {
            thread_local! {
                static REGISTRY: [Compiled; LANGUAGES.len()] = const { [const { OnceLock::new() }; LANGUAGES.len()] };
            }
            REGISTRY.with(|registry| read(registry))
        }
    }
    _ => {
        fn with_registry<T>(read: impl FnOnce(&[Compiled]) -> T) -> T {
            static REGISTRY: [Compiled; LANGUAGES.len()] = [const { OnceLock::new() }; LANGUAGES.len()];
            read(&REGISTRY)
        }
    }
}

pub(crate) fn grammar(language: LanguageId) -> Option<SharedGrammar<Grammar>> {
    with_registry(|registry| {
        registry[language.index()]
            .get_or_init(|| compile(language.language()))
            .clone()
    })
}

fn compile(&Language { name, spec, .. }: &Language) -> Option<SharedGrammar<Grammar>> {
    let (language, highlights, injections) = spec();
    let highlights = if name == "solidity" {
        Cow::Owned(highlights.replace(
            "(struct_expression type: ((expression (identifier)) @type .))",
            "(struct_expression type: (expression (identifier)) @type .)",
        ))
    } else {
        Cow::Borrowed(highlights)
    };
    let query_words: HashSet<Box<str>> = [highlights.as_ref(), injections]
        .into_iter()
        .flat_map(query_words)
        .map(String::into_boxed_str)
        .collect();
    let queried_kinds = (0..language.node_kind_count())
        .map(|_| OnceLock::new())
        .collect();
    let highlights = Query::new(&language, &highlights).ok()?;
    let injections = Query::new(&language, injections).ok()?;
    let tags = TagsGrammar::new(name, &language).ok()?;
    let non_local = [&highlights, &injections]
        .into_iter()
        .any(|query| (0..query.pattern_count()).any(|index| query.is_pattern_non_local(index)));
    let captures = highlights
        .capture_names()
        .iter()
        .map(|name| CaptureRole::of(name))
        .collect();
    let error_prefix = !non_local && !query_words.contains("ERROR") && !query_words.contains("_");
    Some(SharedGrammar::new(Grammar {
        language,
        highlights,
        captures,
        injections,
        non_local,
        error_prefix,
        query_words,
        queried_kinds,
        tags,
    }))
}

/// Shared state for one append across a document and its injections.
pub(crate) struct AppendContext<'a> {
    pub stats: &'a mut SyntaxWorkStats,
    /// Line starts of the root stream, for Tree-sitter edit positions.
    pub line_starts: &'a [usize],
}

pub(crate) struct IncrementalDocument {
    parser: Option<Parser>,
    tree: Option<Tree>,
    grammar: SharedGrammar<Grammar>,
    length: usize,
    spans: Vec<Span>,
    injections: Vec<InjectedDocument>,
    boundaries: Vec<(u32, u32)>,
}

impl IncrementalDocument {
    /// Creates an empty document, or `None` when no grammar is bundled.
    pub(crate) fn new(language: LanguageId) -> Option<Self> {
        grammar(language).map(|grammar| Self {
            parser: None,
            tree: None,
            grammar,
            length: 0,
            spans: Vec::new(),
            injections: Vec::new(),
            boundaries: Vec::new(),
        })
    }

    pub(crate) const fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    pub(crate) fn grammar(&self) -> &Grammar {
        &self.grammar
    }

    /// Appends the bytes of `source` beyond the previous length. `base` is the
    /// document's offset within the root stream.
    pub(crate) fn append(
        &mut self,
        source: &str,
        base: usize,
        depth: usize,
        cx: &mut AppendContext<'_>,
    ) -> Result<usize, SyntaxError> {
        if source.len() == self.length && self.tree.is_some() {
            return Ok(source.len());
        }
        let tree = self.parse_tree(source, base, cx)?;
        let mut start = if self.tree.is_none() || self.grammar.non_local {
            0
        } else {
            self.length
        };
        if let Some(previous) = &self.tree {
            for change in previous.changed_ranges(&tree) {
                let refined = if change.start_byte == 0 {
                    self.grammar
                        .error_prefix(previous.root_node(), tree.root_node(), cx.stats)
                } else {
                    None
                };
                start = start.min(refined.unwrap_or(change.start_byte));
            }
        }
        // A capture ending exactly at the edit point may be an unterminated
        // token that the appended text extends, so widen to it as well.
        loop {
            let first = self
                .boundaries
                .partition_point(|&(end, _)| (end as usize) < start);
            let earlier = self.boundaries[first..]
                .iter()
                .map(|&(_, begin)| begin as usize)
                .min()
                .unwrap_or(start);
            if earlier >= start {
                break;
            }
            start = earlier;
        }
        let (mut spans, mut injections) = loop {
            let result = query(&self.grammar, &tree, source, start, cx.stats);
            let capture_start = result
                .0
                .iter()
                .map(|span| span.start as usize)
                .chain(result.1.iter().map(|injection| injection.start as usize))
                .min()
                .unwrap_or(start);
            if capture_start < start {
                start = capture_start;
            } else {
                break result;
            }
        };
        self.update_boundaries(start, &spans, &injections);
        spans.retain(|span| span.highlight.is_some());
        spans.sort_by_key(|span| span.start);
        injections.sort_by_key(|injection| injection.start);
        let kept = self
            .spans
            .partition_point(|span| (span.start as usize) < start);
        self.spans.truncate(kept);
        self.spans.extend(spans);
        self.update_injections(source, base, start, injections, depth, cx)?;
        self.tree = Some(tree);
        self.length = source.len();
        Ok(start)
    }

    fn update_boundaries(&mut self, start: usize, spans: &[Span], injections: &[Injection]) {
        let kept = self
            .boundaries
            .partition_point(|&(end, _)| end as usize <= start);
        self.boundaries.truncate(kept);
        let mut added: Vec<(u32, u32)> = spans
            .iter()
            .map(|span| (span.end, span.start))
            .chain(
                injections
                    .iter()
                    .map(|injection| (injection.end, injection.start)),
            )
            .collect();
        added.sort_unstable();
        for (end, begin) in added {
            match self.boundaries.last_mut() {
                Some(last) if last.0 == end => last.1 = last.1.min(begin),
                _ => self.boundaries.push((end, begin)),
            }
        }
    }

    fn parse_tree(
        &mut self,
        source: &str,
        base: usize,
        cx: &mut AppendContext<'_>,
    ) -> Result<Tree, SyntaxError> {
        if self.parser.is_none() {
            let mut parser = Parser::new();
            parser.set_language(&self.grammar.language)?;
            self.parser = Some(parser);
        }
        if let Some(tree) = &mut self.tree {
            let old_end = end_point(cx.line_starts, base, self.length);
            tree.edit(&InputEdit {
                start_byte: self.length,
                old_end_byte: self.length,
                new_end_byte: source.len(),
                start_position: old_end,
                old_end_position: old_end,
                new_end_position: end_point(cx.line_starts, base, source.len()),
            });
            cx.stats.incremental_parses += 1;
        } else {
            cx.stats.full_parses += 1;
        }
        let bytes = source.as_bytes();
        let stats = &mut *cx.stats;
        self.parser
            .as_mut()
            .expect("initialized parser")
            .parse_with_options(
                &mut |offset, _| {
                    let end = offset.saturating_add(PARSER_CHUNK_BYTES).min(bytes.len());
                    let input = &bytes[offset.min(bytes.len())..end];
                    stats.parser_input_bytes += input.len();
                    input
                },
                self.tree.as_ref(),
                None,
            )
            .ok_or(SyntaxError::NoTree)
    }

    fn update_injections(
        &mut self,
        source: &str,
        base: usize,
        start: usize,
        injections: Vec<Injection>,
        depth: usize,
        cx: &mut AppendContext<'_>,
    ) -> Result<(), SyntaxError> {
        let kept = self
            .injections
            .partition_point(|entry| entry.offset < start);
        let mut previous = self.injections.split_off(kept);
        if depth > 0 {
            for injection in injections {
                let from = injection.start as usize;
                let to = injection.end as usize;
                let Some(text) = source.get(from..to).filter(|text| !text.is_empty()) else {
                    continue;
                };
                let Some(language) = injection.language.or_else(|| shebang_id(text)) else {
                    continue;
                };
                let reused = previous
                    .iter()
                    .position(|entry| {
                        entry.offset == from
                            && entry.language == language
                            && text.len() >= entry.document.length
                    })
                    .map(|index| previous.swap_remove(index));
                let mut entry = match reused {
                    Some(entry) => entry,
                    None => match Self::new(language) {
                        Some(document) => InjectedDocument {
                            offset: from,
                            language,
                            document,
                        },
                        None => continue,
                    },
                };
                entry.document.append(text, base + from, depth - 1, cx)?;
                self.injections.push(entry);
            }
        }
        Ok(())
    }

    pub(crate) fn spans_from(&self, start: usize, output: &mut Vec<Span>) {
        let first = self
            .spans
            .partition_point(|span| (span.start as usize) < start);
        output.extend_from_slice(&self.spans[first..]);
        let first = self
            .injections
            .partition_point(|entry| entry.offset < start);
        for injection in &self.injections[first..] {
            let offset = u32::try_from(injection.offset).expect("source limit fits u32");
            let from = output.len();
            injection.document.spans_from(0, output);
            for span in &mut output[from..] {
                span.start += offset;
                span.end += offset;
            }
        }
    }
}

impl Clone for IncrementalDocument {
    fn clone(&self) -> Self {
        Self {
            parser: None,
            tree: self.tree.clone(),
            grammar: SharedGrammar::clone(&self.grammar),
            length: self.length,
            spans: self.spans.clone(),
            injections: self.injections.clone(),
            boundaries: self.boundaries.clone(),
        }
    }
}

impl fmt::Debug for IncrementalDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IncrementalDocument")
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct InjectedDocument {
    offset: usize,
    language: LanguageId,
    document: IncrementalDocument,
}

#[derive(Clone, Copy)]
enum CaptureRole {
    Skip,
    Highlight(Option<HighlightId>),
}

impl CaptureRole {
    fn of(name: &str) -> Self {
        if name.starts_with('_') || name.starts_with("injection.") {
            Self::Skip
        } else {
            Self::Highlight(tag_for_capture(name).and_then(HighlightId::from_tag))
        }
    }
}

pub(crate) struct Grammar {
    language: TreeLanguage,
    highlights: Query,
    captures: Box<[CaptureRole]>,
    injections: Query,
    non_local: bool,
    error_prefix: bool,
    query_words: HashSet<Box<str>>,
    queried_kinds: Box<[OnceLock<bool>]>,
    tags: Option<TagsGrammar>,
}

impl Grammar {
    pub(crate) const fn tags(&self) -> Option<&TagsGrammar> {
        self.tags.as_ref()
    }

    fn error_prefix(
        &self,
        before: Node<'_>,
        after: Node<'_>,
        stats: &mut SyntaxWorkStats,
    ) -> Option<usize> {
        if !self.error_prefix {
            return None;
        }
        let before = self.error_root(before)?;
        let after = self.error_root(after)?;
        let mut end = before.start_byte().min(after.start_byte());
        let mut left = before.walk();
        let mut right = after.walk();
        for (before, after) in before.children(&mut left).zip(after.children(&mut right)) {
            if !same_subtree(before, after, stats) {
                break;
            }
            end = before.end_byte().min(after.end_byte());
        }
        Some(end)
    }

    fn error_root<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        if node.is_error() {
            Some(node)
        } else if !self.queries_kind(node) && node.child_count() == 1 {
            node.child(0).filter(Node::is_error)
        } else {
            None
        }
    }

    fn queries_kind(&self, node: Node<'_>) -> bool {
        let queried = || self.query_words.contains(node.kind());
        self.queried_kinds
            .get(usize::from(node.kind_id()))
            .map_or_else(queried, |cell| *cell.get_or_init(queried))
    }
}

fn same_subtree(before: Node<'_>, after: Node<'_>, stats: &mut SyntaxWorkStats) -> bool {
    stats.compared_nodes += 1;
    if before.byte_range() != after.byte_range() || before.has_changes() || after.has_changes() {
        return false;
    }
    if before.id() == after.id() {
        return true;
    }
    if before.kind_id() != after.kind_id() || before.child_count() != after.child_count() {
        return false;
    }
    let mut left = before.walk();
    let mut right = after.walk();
    before
        .children(&mut left)
        .zip(after.children(&mut right))
        .enumerate()
        .all(|(index, (left, right))| {
            let Ok(index) = u32::try_from(index) else {
                return false;
            };
            before.field_name_for_child(index) == after.field_name_for_child(index)
                && same_subtree(left, right, stats)
        })
}

/// Row and column of byte `len` within a document that starts at `base` in
/// the root stream whose line starts are `line_starts`.
fn end_point(line_starts: &[usize], base: usize, len: usize) -> Point {
    let first = line_starts.partition_point(|&start| start <= base);
    let last = line_starts.partition_point(|&start| start <= base + len);
    let column = line_starts[first..last]
        .last()
        .map_or(len, |&start| base + len - start);
    Point::new(last - first, column)
}

fn query(
    grammar: &Grammar,
    tree: &Tree,
    source: &str,
    start: usize,
    stats: &mut SyntaxWorkStats,
) -> (Vec<Span>, Vec<Injection>) {
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(start..source.len());
    stats.queried_bytes += source.len() - start;
    let mut matches = cursor.matches(&grammar.highlights, tree.root_node(), source.as_bytes());
    let mut spans = Vec::new();
    while let Some(found) = matches.next() {
        for capture in found.captures() {
            let CaptureRole::Highlight(highlight) = grammar.captures[capture.index as usize] else {
                continue;
            };
            spans.push(Span {
                start: u32::try_from(capture.node.start_byte()).expect("source limit fits u32"),
                end: u32::try_from(capture.node.end_byte()).expect("source limit fits u32"),
                highlight,
                pattern_index: u32::try_from(found.pattern_index).expect("query pattern fits u32"),
            });
        }
    }
    stats.queried_bytes += source.len() - start;
    let mut matches = cursor.matches(&grammar.injections, tree.root_node(), source.as_bytes());
    let mut injections = Vec::new();
    while let Some(found) = matches.next() {
        let mut content = None;
        let mut language = None;
        for property in grammar.injections.property_settings(found.pattern_index) {
            if property.key.as_ref() == "injection.language" {
                language = property.value.as_deref();
            }
        }
        for capture in found.captures() {
            match grammar.injections.capture_names()[capture.index as usize] {
                "injection.content" => content = Some(capture.node),
                "injection.language" if language.is_none() => {
                    language = capture.node.utf8_text(source.as_bytes()).ok();
                }
                _ => {}
            }
        }
        if let (Some(node), Some(language)) = (content, language) {
            injections.push(Injection {
                start: u32::try_from(node.start_byte()).expect("source limit fits u32"),
                end: u32::try_from(node.end_byte()).expect("source limit fits u32"),
                language: resolve_hint(language),
            });
        }
    }
    (spans, injections)
}

fn query_words(query: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chars = query.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ';' => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                let mut word = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => word.extend(chars.next()),
                        '"' => break,
                        c => word.push(c),
                    }
                }
                words.push(word);
            }
            c if is_word(c) => {
                let mut word = String::from(c);
                while let Some(&c) = chars.peek().filter(|c| is_word(**c)) {
                    word.push(c);
                    chars.next();
                }
                words.push(word);
            }
            _ => {}
        }
    }
    words
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
