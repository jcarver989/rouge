use crate::fonts::{CODE_FONT, UI_FONT};
use crate::syntax::{CODE_WRAP, syntax_spans};
use crate::tokens::Radius;
use crate::typography::{CODE, READING, TITLE, preset};
use crate::{IconName, UiStyles, icon_button};
use rouge_core::Role;
use rouge_core::{
    BuildCx, Color, Corners, Cursor, Div, IntoProp, SpanStyle, StyleDelta, Styled, Text, TextAlign,
    Theme, Typography, Underline, View, component, div, each, rich_text, styled, text, view,
    window_handle,
};
use rouge_markdown::{
    MarkdownBlock, MarkdownBlockKind as Block, MarkdownCodeBlock, MarkdownDocument,
    MarkdownLinkRange, MarkdownListItem, MarkdownMarks as Marks, MarkdownStream,
    MarkdownStreamIdentity, MarkdownTable, MarkdownTableAlignment, MarkdownTargetId, MarkdownText,
};
use rouge_reactive::{Callback, Memo, ReadSignal, Signal, effect, memo, signal};
use rouge_syntax::SyntaxTheme;
use rouge_theme::{MarkdownPalette, Rgba};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct MarkdownPaint {
    palette: Memo<MarkdownPalette>,
    syntax: Memo<SyntaxTheme>,
    on_link: Callback<(String,)>,
    compact: bool,
}

impl MarkdownPaint {
    #[must_use]
    pub fn current() -> Self {
        let theme = Theme::current();
        let rgba = |color: Color| {
            let [r, g, b, a] = color.to_rgba8();
            Rgba::new(r, g, b, a)
        };
        Self {
            palette: memo(move || {
                theme.with(|ui| MarkdownPalette {
                    heading: rgba(ui.text),
                    link: rgba(ui.accent),
                    quote: rgba(ui.border),
                    inline_code: rgba(ui.text),
                    inline_code_background: rgba(ui.surface),
                    code: rgba(ui.text),
                    code_background: rgba(ui.surface),
                })
            }),
            syntax: memo(SyntaxTheme::default),
            on_link: Callback::default(),
            compact: false,
        }
    }

    #[must_use]
    pub fn palette<U>(mut self, palette: impl IntoProp<MarkdownPalette, U>) -> Self {
        let palette = palette.into_prop();
        self.palette = memo(move || palette.get());
        self
    }

    #[must_use]
    pub fn syntax<U>(mut self, syntax: impl IntoProp<SyntaxTheme, U>) -> Self {
        let syntax = syntax.into_prop();
        self.syntax = memo(move || syntax.get());
        self
    }

    #[must_use]
    pub fn on_link(mut self, on_link: impl Into<Callback<(String,)>>) -> Self {
        self.on_link = on_link.into();
        self
    }

    #[must_use]
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    fn prose(&self) -> Div {
        if self.compact {
            prose().px(0.0).body()
        } else {
            prose()
        }
    }

    fn markdown(&self, pick: fn(&MarkdownPalette) -> Rgba) -> impl Fn() -> Color + 'static {
        let palette = self.palette;
        move || palette.with(|palette| Color::from(pick(palette)))
    }
}

pub enum MarkdownSource {
    Document(MarkdownDocument),
    Stream(ReadSignal<MarkdownStream>),
}

impl From<MarkdownDocument> for MarkdownSource {
    fn from(document: MarkdownDocument) -> Self {
        MarkdownSource::Document(document)
    }
}

impl From<ReadSignal<MarkdownStream>> for MarkdownSource {
    fn from(stream: ReadSignal<MarkdownStream>) -> Self {
        MarkdownSource::Stream(stream)
    }
}

impl From<Signal<MarkdownStream>> for MarkdownSource {
    fn from(stream: Signal<MarkdownStream>) -> Self {
        MarkdownSource::Stream(stream.read_only())
    }
}

pub struct Markdown {
    source: MarkdownSource,
    paint: MarkdownPaint,
    style: StyleDelta,
}

#[component]
pub fn markdown(source: impl Into<MarkdownSource>) -> Markdown {
    Markdown {
        source: source.into(),
        paint: MarkdownPaint::current(),
        style: StyleDelta::default(),
    }
}

impl Markdown {
    pub fn on_link(mut self, on_link: impl Into<Callback<(String,)>>) -> Self {
        self.paint = self.paint.on_link(on_link);
        self
    }

    pub fn palette<U>(mut self, palette: impl IntoProp<MarkdownPalette, U>) -> Self {
        self.paint = self.paint.palette(palette);
        self
    }

    pub fn syntax<U>(mut self, syntax: impl IntoProp<SyntaxTheme, U>) -> Self {
        self.paint = self.paint.syntax(syntax);
        self
    }

    pub fn compact(mut self) -> Self {
        self.paint = self.paint.compact();
        self
    }
}

styled!(Markdown => style);

impl View for Markdown {
    fn build(self, cx: &mut BuildCx<'_>) {
        let rows = match self.source {
            MarkdownSource::Document(document) => {
                signal(Rows(MarkdownRows::new(document.blocks()))).read_only()
            }
            MarkdownSource::Stream(stream) => {
                let rows = signal(Rows(MarkdownRows::default()));
                effect(move || stream.with(|stream| rows.update(|rows| rows.0.sync(stream))));
                rows.read_only()
            }
        };
        let paint = self.paint;
        let row = move |entry: ReadSignal<Entry>| {
            let Entry { row, .. } = entry.get_untracked();
            view! { <div pt=row.gap pl=row.indent() flex col>{row.block(&paint)}</div> }
        };
        let root =
            view! { <div flex col min_w=0.0>{each(rows, |entry: &Entry| entry.key, row)}</div> };
        self.style.apply(root).build(cx);
    }
}

struct Rows(MarkdownRows);

impl AsRef<[Entry]> for Rows {
    fn as_ref(&self) -> &[Entry] {
        &self.0.entries
    }
}

#[derive(Default)]
pub struct MarkdownRows {
    entries: Vec<Entry>,
    blocks: Vec<usize>,
    targets: Vec<usize>,
    next_key: u64,
    synced: Option<(MarkdownStreamIdentity, u64)>,
}

#[derive(Clone)]
struct Entry {
    key: u64,
    row: Rc<MarkdownRow>,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl MarkdownRows {
    #[must_use]
    pub fn new(blocks: &[MarkdownBlock]) -> Self {
        let mut rows = Self::default();
        rows.rebuild(blocks, 0, 0);
        rows
    }

    pub fn sync(&mut self, stream: &MarkdownStream) {
        let blocks = stream.document().blocks();
        let current = (stream.identity(), stream.revision());
        match self.synced {
            Some((identity, revision)) if identity == current.0 => {
                if revision != current.1 {
                    let changes = stream.changes_since(revision);
                    self.rebuild(blocks, changes.first_block, changes.first_line);
                }
            }
            _ => self.rebuild(blocks, 0, 0),
        }
        self.synced = Some(current);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&MarkdownRow> {
        self.entries.get(index).map(|entry| &*entry.row)
    }

    #[must_use]
    pub fn index(&self, id: MarkdownTargetId) -> Option<usize> {
        let position = self
            .targets
            .binary_search_by_key(&Some(id), |&row| self.entries[row].row.target)
            .ok()?;
        Some(self.targets[position])
    }

    #[must_use]
    pub fn target(&self, index: usize) -> Option<MarkdownTargetId> {
        self.entries.get(index)?.row.target
    }

    fn rebuild(&mut self, blocks: &[MarkdownBlock], first_block: usize, first_line: usize) {
        let first_block = first_block.min(self.blocks.len()).min(blocks.len());
        let start = self
            .blocks
            .get(first_block)
            .copied()
            .unwrap_or(self.entries.len());
        let kept = self.finished_code_lines(start, first_line);
        self.entries.truncate(start);
        self.blocks.truncate(first_block);
        self.targets
            .truncate(self.targets.partition_point(|&row| row < start));
        let mut rows = Vec::new();
        for block in &blocks[first_block..] {
            self.blocks.push(start + rows.len());
            push_block(block, 0, &mut rows);
        }
        let header = rows
            .first()
            .is_some_and(|row| matches!(row.kind, Kind::CodeHeader { .. }));
        for (offset, mut row) in rows.into_iter().enumerate() {
            let previous = self.entries.last().map(|entry| &entry.row.kind);
            row.gap = previous.map_or(0.0, |previous| gap(previous, &row));
            let key = match offset.checked_sub(1).and_then(|line| kept.get(line)) {
                Some(&key) if header && matches!(row.kind, Kind::CodeLine { .. }) => key,
                _ => {
                    self.next_key += 1;
                    self.next_key
                }
            };
            if row.target.is_some() {
                self.targets.push(self.entries.len());
            }
            self.entries.push(Entry {
                key,
                row: Rc::new(row),
            });
        }
    }

    fn finished_code_lines(&self, start: usize, first_line: usize) -> Vec<u64> {
        let header = self.entries.get(start);
        if !header.is_some_and(|entry| matches!(entry.row.kind, Kind::CodeHeader { .. })) {
            return Vec::new();
        }
        self.entries[start + 1..]
            .iter()
            .take(first_line.saturating_sub(1))
            .take_while(|entry| matches!(entry.row.kind, Kind::CodeLine { .. }))
            .map(|entry| entry.key)
            .collect()
    }
}

pub struct MarkdownRow {
    pub target: Option<MarkdownTargetId>,
    pub gap: f32,
    depth: u16,
    kind: Kind,
}

impl MarkdownRow {
    #[must_use]
    pub fn indent(&self) -> f32 {
        LIST_INDENT * f32::from(self.depth)
    }

    #[must_use]
    pub fn first_line_center(&self) -> f32 {
        match &self.kind {
            Kind::Heading { level, .. } => BLOCK_PADDING_Y + line_height(heading(*level)) / 2.0,
            Kind::ListItem { .. } => line_height(READING) / 2.0,
            Kind::Quote { rows } => rows
                .first()
                .map_or(line_height(READING) / 2.0, Self::first_line_center),
            Kind::CodeHeader { .. } => CODE_HEADER_HEIGHT / 2.0,
            Kind::CodeLine { .. } => line_height(CODE) / 2.0,
            Kind::TableRow { .. } => TABLE_CELL_PADDING_Y + line_height(TABLE) / 2.0,
            Kind::Paragraph { .. } | Kind::Html { .. } | Kind::Rule => {
                BLOCK_PADDING_Y + line_height(READING) / 2.0
            }
        }
    }

    pub fn block(&self, paint: &MarkdownPaint) -> Div {
        match &self.kind {
            Kind::Heading { level, content } => paint
                .prose()
                .typography(if paint.compact {
                    TITLE
                } else {
                    heading(*level)
                })
                .text_color(paint.markdown(|m| m.heading))
                .child(inline_text(content, paint)),
            Kind::Paragraph { content } => paint.prose().child(inline_text(content, paint)),
            Kind::ListItem { marker, content } => paint
                .prose()
                .py(0.0)
                .flex()
                .row()
                .gap(8.0)
                .child(view! {
                    <div min_w=16.0 flex_none muted>
                        <text>{marker.clone()}</text>
                    </div>
                })
                .child(view! {
                    <div flex_1 min_w=0.0>{inline_text(content, paint)}</div>
                }),
            Kind::Quote { rows } => view! {
                <div px=8.0>
                    <div pl=8.0 border_l=2.0 border_color=paint.markdown(|m| m.quote)
                         secondary flex col>
                        {..rows.iter().map(|row| view! {
                            <div pt=row.gap pl=row.indent() flex col>{row.block(paint)}</div>
                        })}
                    </div>
                </div>
            },
            Kind::CodeHeader { code } if paint.compact => {
                code_cap(code).bg(paint.markdown(|m| m.code_background))
            }
            Kind::CodeHeader { code } => {
                code_header(code).bg(paint.markdown(|m| m.code_background))
            }
            Kind::CodeLine { code, index } => {
                code_line(code, *index, paint).bg(paint.markdown(|m| m.code_background))
            }
            Kind::TableRow {
                cells,
                alignments,
                header,
                first,
                last,
            } => {
                let row = table_row(cells, alignments, *header, *first, *last, paint);
                if *header { row.bg(Role::Surface) } else { row }
            }
            Kind::Rule => view! {
                <div py=12.0><div w_full border_t=1.0 border_color=Role::Border/></div>
            },
            Kind::Html { content } => paint
                .prose()
                .muted()
                .child(inline_text(content, paint).code()),
        }
    }
}

const LIST_INDENT: f32 = 22.0;
const BLOCK_PADDING_Y: f32 = 4.0;

const TABLE_CELL_PADDING_Y: f32 = 8.0;
const CODE_HEADER_HEIGHT: f32 = 32.0;
const TABLE: Typography = preset(UI_FONT, 14.0, 20.0, 400.0);

const fn heading(level: u8) -> Typography {
    match level {
        1 => preset(UI_FONT, 26.0, 34.0, 600.0),
        2 => preset(UI_FONT, 21.0, 30.0, 600.0),
        3 => preset(UI_FONT, 17.0, 26.0, 600.0),
        _ => preset(UI_FONT, 15.0, 24.0, 600.0),
    }
}

fn line_height(typography: Typography) -> f32 {
    (typography.size * typography.line_height).round()
}

enum Kind {
    Heading {
        level: u8,
        content: Arc<MarkdownText>,
    },
    Paragraph {
        content: Arc<MarkdownText>,
    },
    ListItem {
        marker: String,
        content: Arc<MarkdownText>,
    },
    Quote {
        rows: Vec<MarkdownRow>,
    },
    CodeHeader {
        code: Arc<MarkdownCodeBlock>,
    },
    CodeLine {
        code: Arc<MarkdownCodeBlock>,
        index: usize,
    },
    TableRow {
        cells: Vec<Arc<MarkdownText>>,
        alignments: Rc<[MarkdownTableAlignment]>,
        header: bool,
        first: bool,
        last: bool,
    },
    Rule,
    Html {
        content: Arc<MarkdownText>,
    },
}

#[component]
fn prose() -> Div {
    div()
        .px(8.0)
        .py(BLOCK_PADDING_Y)
        .rounded(Radius::Inner)
        .min_w(0.0)
        .reading()
}

fn corners(top: bool, bottom: bool) -> Corners {
    let (top, bottom) = (
        if top { Radius::Control.px() } else { 0.0 },
        if bottom { Radius::Control.px() } else { 0.0 },
    );
    Corners {
        top_left: top,
        top_right: top,
        bottom_right: bottom,
        bottom_left: bottom,
    }
}

fn code_cap(code: &MarkdownCodeBlock) -> Div {
    let empty = code.lines().is_empty();
    let cap = view! {
        <div h=8.0 border_t=1.0 border_l=1.0 border_r=1.0 border_color=Role::Border
             rounded=corners(true, empty)/>
    };
    if empty { cap.border_b(1.0) } else { cap }
}

fn code_header(code: &Arc<MarkdownCodeBlock>) -> Div {
    let empty = code.lines().is_empty();
    let (code, window) = (Arc::clone(code), window_handle());
    let header = view! {
        <div h=CODE_HEADER_HEIGHT pl=16.0 pr=4.0 flex row items_center justify_between border_t=1.0
             border_l=1.0 border_r=1.0 border_color=Role::Border rounded=corners(true, empty)>
            <text caption muted>{code.language().unwrap_or("code").to_owned()}</text>
            <icon_button test_id="markdown-copy-code" icon=IconName::Copy label="Copy code"
                         on_click={
                             let code = Arc::clone(&code);
                             move || {
                                 let text = if code.lines().is_empty() {
                                     String::new()
                                 } else {
                                     format!("{}\n", code.text())
                                 };
                                 window.copy_text(text);
                             }
                         }/>
        </div>
    };
    if empty { header.border_b(1.0) } else { header }
}

fn code_line(code: &MarkdownCodeBlock, index: usize, paint: &MarkdownPaint) -> Div {
    let lines = code.lines();
    let last = index + 1 == lines.len();
    let source = lines
        .get(index)
        .map_or_else(String::new, |line| code.line_text(line).to_owned());
    let (syntax, highlights) = (paint.syntax, Arc::clone(&code.syntax().highlights));
    let spans = move || {
        let line = highlights.line(index).unwrap_or_default();
        syntax.with(|syntax| syntax_spans(line, syntax))
    };
    let line = view! {
        <div px=16.0 min_h=line_height(CODE) overflow_hidden border_l=1.0 border_r=1.0
             border_color=Role::Border rounded=corners(false, last)>
            <rich_text spans code wrap_indent=CODE_WRAP color=paint.markdown(|m| m.code)>{source}</rich_text>
        </div>
    };
    if last {
        line.pb(8.0).border_b(1.0)
    } else {
        line
    }
}

fn table_row(
    cells: &[Arc<MarkdownText>],
    alignments: &[MarkdownTableAlignment],
    header: bool,
    first: bool,
    last: bool,
    paint: &MarkdownPaint,
) -> Div {
    let cells = cells.iter().enumerate().map(|(column, cell)| {
        let align = match alignments.get(column) {
            Some(MarkdownTableAlignment::Center) => TextAlign::Center,
            Some(MarkdownTableAlignment::Right) => TextAlign::End,
            _ => TextAlign::Start,
        };
        let cell = view! {
            <div flex_1 min_w=0.0 px=12.0 py=TABLE_CELL_PADDING_Y>{inline_text(cell, paint).align(align)}</div>
        };
        if column > 0 {
            cell.border_l(1.0).border_color(Role::Border)
        } else {
            cell
        }
    });
    let row = view! {
        <div flex row border_l=1.0 border_r=1.0 border_b=1.0 border_color=Role::Border
             rounded=corners(first, last) typography=TABLE>
            {..cells}
        </div>
    };
    let row = if header { row.font_weight(600.0) } else { row };
    if first { row.border_t(1.0) } else { row }
}

fn mark_style(marks: Marks, markdown: &MarkdownPalette, muted: Color) -> SpanStyle {
    let link = Color::from(markdown.link);
    let code = marks.contains(Marks::CODE);
    let foreground = if marks.contains(Marks::LINK) {
        Some(link)
    } else if code {
        Some(Color::from(markdown.inline_code))
    } else if marks.contains(Marks::IMAGE) {
        Some(muted)
    } else {
        None
    };
    SpanStyle {
        color: foreground,
        background: code.then(|| Color::from(markdown.inline_code_background)),
        weight: marks.contains(Marks::STRONG).then_some(600.0),
        italic: (marks.contains(Marks::EMPHASIS) || marks.contains(Marks::IMAGE)).then_some(true),
        underline: marks.contains(Marks::LINK).then_some(Underline {
            color: link,
            thickness: 1.0,
        }),
        strikethrough: marks.contains(Marks::STRIKETHROUGH).then_some(true),
        family: code.then(|| CODE_FONT.into()),
    }
}

fn inline_text(content: &Arc<MarkdownText>, paint: &MarkdownPaint) -> Text {
    let source = content.text().to_owned();
    let text = if content.marks().is_empty() {
        text(source)
    } else {
        let (palette, muted) = (paint.palette, Theme::current().color(Role::TextMuted));
        let marked = Arc::clone(content);
        let spans = move || {
            let muted = muted.get();
            palette.with(|palette| {
                let marks = marked.marks().iter();
                marks
                    .map(|mark| (mark.range.clone(), mark_style(mark.marks, palette, muted)))
                    .collect()
            })
        };
        rich_text(source, spans)
    };
    if !content.links().iter().any(MarkdownLinkRange::openable) {
        return text;
    }
    let hovered = signal(false);
    let on_link = paint.on_link;
    let (hover, click) = (Arc::clone(content), Arc::clone(content));
    text.cursor(move || hovered.get().then_some(Cursor::Pointer))
        .on_range_hover(move |index| {
            hovered.set_if_changed(openable_link(&hover, index).is_some());
        })
        .on_range_click(move |index| {
            if let Some(link) = openable_link(&click, index) {
                on_link.call(link.destination.clone());
            }
        })
}

fn openable_link(text: &MarkdownText, index: Option<usize>) -> Option<&MarkdownLinkRange> {
    text.link_at(index?).filter(|link| link.openable())
}

fn push_block(block: &MarkdownBlock, depth: u16, rows: &mut Vec<MarkdownRow>) {
    let kind = match &block.kind {
        Block::Heading { level, content } => Kind::Heading {
            level: *level,
            content: Arc::clone(content),
        },
        Block::Paragraph { content } => Kind::Paragraph {
            content: Arc::clone(content),
        },
        Block::BlockQuote { blocks } => Kind::Quote {
            rows: nested_rows(blocks),
        },
        Block::HtmlFallback { content } => Kind::Html {
            content: Arc::clone(content),
        },
        Block::Rule => Kind::Rule,
        Block::List {
            ordered,
            start,
            items,
        } => return push_list(*ordered, *start, items, depth, rows),
        Block::CodeBlock(code) => return push_code(code, depth, rows),
        Block::Table(table) => return push_table(table, depth, rows),
    };
    rows.push(MarkdownRow {
        target: block.target_id,
        depth,
        gap: 0.0,
        kind,
    });
}

fn nested_rows(blocks: &[MarkdownBlock]) -> Vec<MarkdownRow> {
    let mut rows = Vec::new();
    for block in blocks {
        push_block(block, 0, &mut rows);
    }
    for index in 1..rows.len() {
        rows[index].gap = gap(&rows[index - 1].kind, &rows[index]);
    }
    rows
}

fn push_list(
    ordered: bool,
    start: Option<u64>,
    items: &[MarkdownListItem],
    depth: u16,
    rows: &mut Vec<MarkdownRow>,
) {
    let first = start.unwrap_or(1);
    for (offset, item) in (0_u64..).zip(items) {
        let marker = if ordered {
            format!("{}.", first + offset)
        } else {
            bullet(depth).to_owned()
        };
        rows.push(MarkdownRow {
            target: item.target_id,
            depth,
            gap: 0.0,
            kind: Kind::ListItem {
                marker,
                content: Arc::clone(&item.content),
            },
        });
        for block in &item.blocks {
            push_block(block, depth.saturating_add(1), rows);
        }
    }
}

fn push_code(code: &Arc<MarkdownCodeBlock>, depth: u16, rows: &mut Vec<MarkdownRow>) {
    rows.push(MarkdownRow {
        target: code.target_id,
        depth,
        gap: 0.0,
        kind: Kind::CodeHeader {
            code: Arc::clone(code),
        },
    });
    rows.extend(
        code.lines()
            .iter()
            .enumerate()
            .map(|(index, line)| MarkdownRow {
                target: line.target_id,
                depth,
                gap: 0.0,
                kind: Kind::CodeLine {
                    code: Arc::clone(code),
                    index,
                },
            }),
    );
}

fn push_table(table: &MarkdownTable, depth: u16, rows: &mut Vec<MarkdownRow>) {
    let alignments: Rc<[MarkdownTableAlignment]> = Rc::from(table.alignments.as_slice());
    let count = table.rows.len();
    rows.extend(table.rows.iter().enumerate().map(|(index, row)| {
        MarkdownRow {
            target: row.target_id,
            depth,
            gap: 0.0,
            kind: Kind::TableRow {
                cells: row
                    .cells
                    .iter()
                    .map(|cell| Arc::clone(&cell.content))
                    .collect(),
                alignments: alignments.clone(),
                header: row.header,
                first: index == 0,
                last: index + 1 == count,
            },
        }
    }));
}

fn gap(previous: &Kind, row: &MarkdownRow) -> f32 {
    match (&row.kind, previous) {
        (Kind::CodeLine { .. } | Kind::TableRow { first: false, .. }, _) => 0.0,
        (Kind::Heading { level: 1, .. }, _) => 36.0,
        (Kind::Heading { level: 2, .. }, _) => 32.0,
        (Kind::Heading { .. }, _) => 24.0,
        (_, Kind::Heading { .. }) => 8.0,
        (Kind::ListItem { .. }, Kind::ListItem { .. }) => 4.0,
        (_, _) if row.depth > 0 => 6.0,
        (Kind::Rule, _) => 20.0,
        _ => 14.0,
    }
}

const fn bullet(depth: u16) -> &'static str {
    match depth % 3 {
        0 => "•",
        1 => "◦",
        _ => "▪",
    }
}
