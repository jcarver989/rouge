use crate::fonts::{CODE_FONT, UI_FONT};
use crate::syntax::{CODE_WRAP, syntax_spans};
use crate::tokens::{RADIUS_CONTROL, RADIUS_INNER};
use crate::typography::{CODE, READING, preset};
use crate::{IconName, UiStyles, icon_button};
use rouge_core::{
    Color, Corners, Cursor, Div, SpanStyle, Styled, Text, TextAlign, Theme, Typography, Underline,
    View, component, div, each, rich_text, show, text, view,
};
use rouge_markdown::{
    MarkdownBlock, MarkdownBlockKind as Block, MarkdownCodeBlock, MarkdownInline,
    MarkdownLinkRange, MarkdownListItem, MarkdownMarks as Marks, MarkdownStream,
    MarkdownStreamIdentity, MarkdownTable, MarkdownTableAlignment, MarkdownTargetId, MarkdownText,
};
use rouge_reactive::{Callback, Memo, ReadSignal, memo, signal};
use rouge_syntax::SyntaxTheme;
use rouge_theme::{MarkdownPalette, Rgba};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownAction {
    OpenLink(String),
    CopyCode(String),
}

#[derive(Clone, Copy)]
pub struct MarkdownPaint {
    palette: Memo<MarkdownPalette>,
    syntax: Memo<SyntaxTheme>,
    clipboard: Memo<bool>,
    on_action: Callback<(MarkdownAction,)>,
}

impl MarkdownPaint {
    pub fn new(
        palette: Memo<MarkdownPalette>,
        syntax: Memo<SyntaxTheme>,
        on_action: impl Fn(MarkdownAction) + 'static,
    ) -> Self {
        Self {
            palette,
            syntax,
            clipboard: memo(|| true),
            on_action: on_action.into(),
        }
    }

    #[must_use]
    pub fn clipboard(mut self, clipboard: Memo<bool>) -> Self {
        self.clipboard = clipboard;
        self
    }

    fn markdown(&self, pick: fn(&MarkdownPalette) -> Rgba) -> impl Fn() -> Color + 'static {
        let palette = self.palette;
        move || palette.with(|palette| Color::from(pick(palette)))
    }
}

#[component]
pub fn markdown(rows: ReadSignal<MarkdownRows>, paint: MarkdownPaint) -> impl View {
    let entries = move || rows.with(|rows| rows.entries.clone());
    let row = move |entry: ReadSignal<Entry>| {
        let Entry { row, .. } = entry.get_untracked();
        view! {
            <div pt=row.gap pl=row.indent() flex col>{row.block(&paint).bg(row.background(&paint))}</div>
        }
    };
    view! { <div flex col min_w=0.0>{each(entries, |entry: &Entry| entry.key, row)}</div> }
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
            Kind::ListItem { .. } => 1.0 + line_height(READING) / 2.0,
            Kind::Quote { rows } => rows
                .first()
                .map_or(line_height(READING) / 2.0, Self::first_line_center),
            Kind::CodeHeader { .. } => CODE_HEADER_HEIGHT / 2.0,
            Kind::CodeLine { .. } => line_height(CODE) / 2.0,
            Kind::TableRow { .. } => 6.0 + line_height(TABLE) / 2.0,
            Kind::Paragraph { .. } | Kind::Html { .. } | Kind::Rule => {
                BLOCK_PADDING_Y + line_height(READING) / 2.0
            }
        }
    }

    pub fn background(&self, paint: &MarkdownPaint) -> Box<dyn Fn() -> Color> {
        match &self.kind {
            Kind::CodeHeader { .. } | Kind::CodeLine { .. } => {
                Box::new(paint.markdown(|m| m.code_background))
            }
            Kind::TableRow { header: true, .. } => {
                let surface = Theme::current().surface();
                Box::new(move || surface.get())
            }
            _ => Box::new(|| Color::TRANSPARENT),
        }
    }

    pub fn block(&self, paint: &MarkdownPaint) -> Div {
        let theme = Theme::current();
        match &self.kind {
            Kind::Heading { level, content } => prose()
                .typography(heading(*level))
                .text_color(paint.markdown(|m| m.heading))
                .child(inline_text(content, paint)),
            Kind::Paragraph { content } => view! { <prose>{inline_text(content, paint)}</prose> },
            Kind::ListItem { marker, content } => view! {
                <prose py=1.0 flex row gap=8.0>
                    <div min_w=14.0 flex_none muted>
                        <text>{marker.clone()}</text>
                    </div>
                    <div flex_1 min_w=0.0>{inline_text(content, paint)}</div>
                </prose>
            },
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
            Kind::CodeHeader { code } => code_header(code, paint),
            Kind::CodeLine { code, index } => code_line(code, *index, paint),
            Kind::TableRow {
                cells,
                alignments,
                header,
                first,
                last,
            } => table_row(cells, alignments, *header, *first, *last, paint),
            Kind::Rule => view! {
                <div py=12.0><div h=1.0 w_full bg=theme.border()/></div>
            },
            Kind::Html { content } => view! {
                <prose muted>{inline_text(content, paint).code()}</prose>
            },
        }
    }
}

const LIST_INDENT: f32 = 22.0;
const BLOCK_PADDING_Y: f32 = 4.0;
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
        content: Rc<MarkdownText>,
    },
    Paragraph {
        content: Rc<MarkdownText>,
    },
    ListItem {
        marker: String,
        content: Rc<MarkdownText>,
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
        cells: Vec<Rc<MarkdownText>>,
        alignments: Rc<[MarkdownTableAlignment]>,
        header: bool,
        first: bool,
        last: bool,
    },
    Rule,
    Html {
        content: Rc<MarkdownText>,
    },
}

#[component]
fn prose() -> Div {
    div()
        .px(8.0)
        .py(BLOCK_PADDING_Y)
        .rounded(RADIUS_INNER)
        .min_w(0.0)
        .reading()
}

fn corners(top: bool, bottom: bool) -> Corners {
    let (top, bottom) = (
        if top { RADIUS_CONTROL } else { 0.0 },
        if bottom { RADIUS_CONTROL } else { 0.0 },
    );
    Corners {
        top_left: top,
        top_right: top,
        bottom_right: bottom,
        bottom_left: bottom,
    }
}

fn code_header(code: &Arc<MarkdownCodeBlock>, paint: &MarkdownPaint) -> Div {
    let empty = code.lines().is_empty();
    let clipboard = paint.clipboard;
    let (code, on_action, theme) = (Arc::clone(code), paint.on_action, Theme::current());
    let header = view! {
        <div h=CODE_HEADER_HEIGHT pl=16.0 pr=4.0 flex row items_center justify_between border_t=1.0
             border_l=1.0 border_r=1.0 border_color=theme.border() rounded=corners(true, empty)>
            <text caption muted>{code.language().unwrap_or("code").to_owned()}</text>
            <show when={move || clipboard.get()}>
                <icon_button id="markdown-copy-code" icon=IconName::Copy label="Copy code"
                             on_click={
                                 let code = Arc::clone(&code);
                                 move || {
                                     let text = if code.lines().is_empty() {
                                         String::new()
                                     } else {
                                         format!("{}\n", code.text())
                                     };
                                     on_action.call(MarkdownAction::CopyCode(text));
                                 }
                             }/>
            </show>
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
             border_color=Theme::current().border() rounded=corners(false, last)>
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
    cells: &[Rc<MarkdownText>],
    alignments: &[MarkdownTableAlignment],
    header: bool,
    first: bool,
    last: bool,
    paint: &MarkdownPaint,
) -> Div {
    let theme = Theme::current();
    let cells = cells.iter().enumerate().map(|(column, cell)| {
        let align = match alignments.get(column) {
            Some(MarkdownTableAlignment::Center) => TextAlign::Center,
            Some(MarkdownTableAlignment::Right) => TextAlign::End,
            _ => TextAlign::Start,
        };
        let cell = view! {
            <div flex_1 min_w=0.0 px=12.0 py=6.0>{inline_text(cell, paint).align(align)}</div>
        };
        if column > 0 {
            cell.border_l(1.0).border_color(theme.border())
        } else {
            cell
        }
    });
    let row = view! {
        <div flex row border_l=1.0 border_r=1.0 border_b=1.0 border_color=theme.border()
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

fn inline_text(content: &Rc<MarkdownText>, paint: &MarkdownPaint) -> Text {
    let source = content.text().to_owned();
    let text = if content.marks().is_empty() {
        text(source)
    } else {
        let (palette, muted) = (paint.palette, Theme::current().text_muted());
        let marked = Rc::clone(content);
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
    let on_action = paint.on_action;
    let (hover, click) = (Rc::clone(content), Rc::clone(content));
    text.cursor(move || hovered.get().then_some(Cursor::Pointer))
        .on_range_hover(move |index| {
            hovered.set_if_changed(openable_link(&hover, index).is_some());
        })
        .on_range_click(move |index| {
            if let Some(link) = openable_link(&click, index) {
                on_action.call(MarkdownAction::OpenLink(link.destination.clone()));
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
            content: flatten(content),
        },
        Block::Paragraph { content } => Kind::Paragraph {
            content: flatten(content),
        },
        Block::BlockQuote { blocks } => Kind::Quote {
            rows: nested_rows(blocks),
        },
        Block::HtmlFallback { content } => Kind::Html {
            content: flatten(content),
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

fn flatten(content: &[MarkdownInline]) -> Rc<MarkdownText> {
    Rc::new(MarkdownText::new(content))
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
                content: flatten(&item.content),
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
    rows.extend(code.lines().iter().map(|line| MarkdownRow {
        target: line.target_id,
        depth,
        gap: 0.0,
        kind: Kind::CodeLine {
            code: Arc::clone(code),
            index: line.index,
        },
    }));
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
                    .map(|cell| flatten(&cell.content))
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
