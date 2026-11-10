mod patch;
mod presentation;
mod syntax;
pub(crate) mod theme;

pub(crate) use patch::Patch;

use patch::{FileDiff, Status, Tone};
use presentation::{Cell, Document, HEADER_HEIGHT, Presentation, Row, RowKey, RowKind};
use rouge::core::IntoProp;
use rouge::prelude::*;
use std::ops::Range;
use std::rc::Rc;
use theme::{ACCENT, BASE, CODE_FONT, FAINT, GREEN, HUNK, MUTED, RED, SELECTION, TEXT, UI_FONT};

pub(crate) const SAMPLE: &str = include_str!("sample.diff");

const MARGIN: f32 = 10.0;
const TOOLBAR_HEIGHT: f32 = 44.0;
const CHROME: f32 = MARGIN + TOOLBAR_HEIGHT + 6.0;
const HUD_WIDTH: f32 = 440.0;
const HUD_HEIGHT: f32 = 44.0;
const PANEL_GLASS: Glass = Glass::new()
    .blur(6.0)
    .edge_width(14.0)
    .tint(FROST)
    .highlight(0.55)
    .shadow(0.35);
const HEADER_GLASS: Glass = Glass::new()
    .blur(6.0)
    .edge_width(8.0)
    .tint(FROST)
    .highlight(0.4)
    .shadow(0.2);
const HUD_GLASS: Glass = Glass::new()
    .blur(3.0)
    .edge_width(16.0)
    .refraction(0.9)
    .tint(FROST)
    .highlight(0.6)
    .shadow(0.4);
const FROST: Color = Color::WHITE.with_alpha(0.04);
const RIM: Color = Color::WHITE.with_alpha(0.14);
const LIFT: Color = Color::WHITE.with_alpha(0.07);
const PRESS: Color = Color::WHITE.with_alpha(0.13);
const SIDEBAR_WIDTH: f32 = 260.0;
const SCROLLBAR_WIDTH: f32 = 10.0;
const INDICATOR_WIDTH: f32 = 4.0;
const GUTTER_WIDTH: f32 = 52.0;
const SEGMENT_WIDTH: f32 = 72.0;
const SEGMENT_HEIGHT: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Old,
    New,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Selection {
    key: RowKey,
    side: Side,
    file: usize,
    number: Option<usize>,
    tone: Tone,
}

#[derive(Clone, Copy)]
struct Review {
    document: ReadSignal<Rc<Document>>,
    split: Signal<bool>,
    collapsed: Signal<Vec<bool>>,
    selected: Signal<Option<Selection>>,
    list: VirtualListHandle,
    commenting: Signal<bool>,
    draft: Signal<String>,
    comments: Signal<Vec<(Selection, String)>>,
    rows: Signal<Range<usize>>,
    presentation: Memo<Rc<Presentation>>,
    offset: Memo<f32>,
    active_file: Memo<usize>,
}

impl Review {
    fn new(document: Document) -> Review {
        let files = document.files().len();
        let document = signal(Rc::new(document)).read_only();
        let split = signal(false);
        let collapsed = signal(vec![false; files]);
        let list = virtual_list_handle();

        let presentation =
            memo(move || Rc::new(document.with(|d| collapsed.with(|c| d.present(split.get(), c)))));
        let offset = memo(move || list.scroll_state().with(|state| state.offset.y.round()));
        let active_file =
            memo(move || presentation.with(|p| p.file_at(offset.get(), |row| list.row_top(row))));
        Review {
            document,
            split,
            collapsed,
            selected: signal(None),
            list,
            commenting: signal(false),
            draft: signal(String::new()),
            comments: signal(Vec::new()),
            rows: signal(0..0),
            presentation,
            offset,
            active_file,
        }
    }

    fn files(self) -> Vec<Rc<FileDiff>> {
        self.document.with_untracked(|document| document.files())
    }

    fn scroll_to_file(self, file: usize) {
        let row = self
            .presentation
            .with_untracked(|p| p.file_rows(file).start);
        self.list.scroll_to_row(row, Align::Top);
    }

    fn toggle_file(self, file: usize) {
        let row = self
            .presentation
            .with_untracked(|p| p.file_rows(file).start);
        let top = self.list.row_top(row);
        self.collapsed
            .update(|collapsed| collapsed[file] = !collapsed[file]);
        if top < self.offset.get_untracked() {
            self.list.scroll_to_row(row, Align::Top);
        }
    }

    fn set_split(self, split: bool) {
        if self.split.get_untracked() == split {
            return;
        }
        let file = self.active_file.get_untracked();
        let previous = self.presentation.with_untracked(|p| p.file_rows(file));
        let row = self
            .list
            .top_row()
            .get_untracked()
            .clamp(previous.start, previous.end.saturating_sub(1));
        self.split.set(split);
        let current = self.presentation.with_untracked(|p| p.file_rows(file));
        let index = current.start + (row - previous.start) * current.len() / previous.len().max(1);
        self.list.scroll_to_row(index, Align::Top);
    }
}

pub(crate) fn diff_view(patch: Patch) -> AnyView {
    if patch.files.is_empty() {
        let message = label("No changes to review", 14.0, MUTED);
        return div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(message)
            .into_any();
    }
    provide_theme(Palette::dark());
    let review = Review::new(Document::new(patch));
    div()
        .size_full()
        .relative()
        .font(UI_FONT)
        .text_size(rem(13.0 / 16.0))
        .text_color(TEXT)
        .bg(BASE)
        .child(diff_pane(review))
        .child(sidebar(review))
        .into_any()
}

fn toolbar(review: Review) -> Div {
    let files = review.files();
    let added: usize = files.iter().map(|file| file.added).sum();
    let removed: usize = files.iter().map(|file| file.removed).sum();
    let in_view = move || {
        let rows = review.rows.get();
        format!(
            "rows {}–{} of {} in view",
            review.list.top_row().get() + 1,
            rows.end,
            review.presentation.with(|p| p.len())
        )
    };
    div()
        .absolute()
        .top(MARGIN)
        .left(MARGIN)
        .right(MARGIN + SCROLLBAR_WIDTH)
        .h(TOOLBAR_HEIGHT)
        .flex()
        .items_center()
        .gap(12.0)
        .pl(16.0)
        .pr(8.0)
        .rounded(TOOLBAR_HEIGHT / 2.0)
        .backdrop(PANEL_GLASS)
        .child(label("Rouge diff", 14.0, TEXT).weight(600.0))
        .child(label(format!("{} files changed", files.len()), 13.0, MUTED))
        .child(stats(added, removed))
        .child(div().flex_1())
        .child(
            div()
                .font(CODE_FONT)
                .text_size(rem(0.75))
                .child(code(in_view, MUTED)),
        )
        .child(show(
            move || review.comments.with(|c| !c.is_empty()),
            move || {
                div().test_id("diff-comments-saved").child(code(
                    move || format!("{} comments", review.comments.with(Vec::len)),
                    GREEN,
                ))
            },
        ))
        .child(layout_switch(review))
}

fn layout_switch(review: Review) -> Div {
    let segment = move |name: &'static str, split: bool| {
        let color = move || {
            if review.split.get() == split {
                TEXT
            } else {
                MUTED
            }
        };
        div()
            .flex()
            .items_center()
            .justify_center()
            .w(SEGMENT_WIDTH)
            .h(SEGMENT_HEIGHT)
            .test_id(if split { "diff-split" } else { "diff-unified" })
            .on_click(move |_| review.set_split(split))
            .child(label(name, 12.0, TEXT).weight(500.0).color(color))
    };
    let pill = div()
        .absolute()
        .top(2.0)
        .left(2.0)
        .w(SEGMENT_WIDTH)
        .h(SEGMENT_HEIGHT)
        .rounded(SEGMENT_HEIGHT / 2.0)
        .bg(PRESS)
        .translate_x(move || {
            if review.split.get() {
                SEGMENT_WIDTH
            } else {
                0.0
            }
        })
        .transition(Spring::snappy());
    div()
        .relative()
        .flex()
        .flex_none()
        .p(2.0)
        .rounded(SEGMENT_HEIGHT / 2.0 + 2.0)
        .bg(Color::BLACK.with_alpha(0.22))
        .child(pill)
        .child(segment("Unified", false))
        .child(segment("Split", true))
}

fn sidebar(review: Review) -> Div {
    let entry = move |(index, file): (usize, Rc<FileDiff>)| {
        let active = memo(move || review.active_file.get() == index);
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(8.0)
            .h(28.0)
            .mx(6.0)
            .px(8.0)
            .rounded(6.0)
            .bg(move || {
                if active.get() {
                    PRESS
                } else {
                    Color::TRANSPARENT
                }
            })
            .hover(|style| style.bg(LIFT))
            .test_id(format!("diff-file-{index}"))
            .on_click(move |_| review.scroll_to_file(index))
            .child(status_code(file.status))
            .child(clipped(label(file.name(), 13.0, TEXT)))
            .child(stats(file.added, file.removed))
    };
    div()
        .absolute()
        .top(MARGIN)
        .left(MARGIN)
        .bottom(MARGIN)
        .w(SIDEBAR_WIDTH - MARGIN)
        .flex()
        .col()
        .gap(2.0)
        .py(14.0)
        .overflow_hidden()
        .rounded(TOOLBAR_HEIGHT / 2.0)
        .backdrop(PANEL_GLASS)
        .child(
            div()
                .px(14.0)
                .pb(6.0)
                .child(label("CHANGED FILES", 11.0, FAINT).weight(600.0)),
        )
        .children(review.files().into_iter().enumerate().map(entry))
}

fn diff_pane(review: Review) -> Div {
    let presentation = review.presentation;
    let rows = virtual_list(
        move || presentation.with(|p| p.len()),
        move |index| presentation.with(|p| p.row(index).map_or(0.0, |row| row.height())),
        move |index| row(review, index),
    )
    .handle(review.list)
    .track_rows(review.rows);
    let rows = div()
        .test_id("diff-scroll")
        .absolute()
        .top(CHROME)
        .left(0.0)
        .right(0.0)
        .bottom(0.0)
        .child(
            scroll_view(rows)
                .scroll_state(review.list.scroll_state())
                .size_full(),
        );
    div()
        .absolute()
        .top(0.0)
        .left(SIDEBAR_WIDTH)
        .right(0.0)
        .bottom(0.0)
        .child(rows)
        .child(sticky_header(review))
        .child(toolbar(review))
        .child(selection_hud(review))
        .child(show(
            move || review.commenting.get(),
            move || comment_editor(review),
        ))
}

fn sticky_header(review: Review) -> Div {
    let push = memo(move || {
        let rows = review
            .presentation
            .with(|p| p.file_rows(review.active_file.get()));
        let bottom = review.list.row_top(rows.end);
        (bottom - review.offset.get() - HEADER_HEIGHT).min(0.0)
    });
    let bar = move || {
        header_bar(review, review.active_file.get())
            .test_id(format!("diff-sticky-file-{}", review.active_file.get()))
            .border_color(RIM)
            .hover(|style| style.bg(LIFT))
            .backdrop(HEADER_GLASS)
    };
    div()
        .test_id("diff-sticky")
        .absolute()
        .top(CHROME)
        .left(0.0)
        .right(SCROLLBAR_WIDTH)
        .h(HEADER_HEIGHT)
        .px(8.0)
        .py(4.0)
        .translate_y(push)
        .opacity(move || 1.0 + push.get() / HEADER_HEIGHT)
        .child(dynamic(bar))
}

fn selection_hud(review: Review) -> Div {
    let shown = memo(move || review.selected.with(Option::is_some) && !review.commenting.get());
    let latest = signal(None);
    effect(move || {
        if let Some(selection) = review.selected.get() {
            latest.set(Some(selection));
        }
    });
    let files = review.files();
    let describe = move || match latest.get() {
        Some(Selection {
            file,
            number,
            tone,
            side,
            ..
        }) => {
            let line = number.map_or_else(String::new, |number| format!(":{number}"));
            let kind = match (tone, side) {
                (Tone::Added, _) => "added",
                (Tone::Removed, _) => "removed",
                (_, Side::Old) => "old side",
                (_, Side::New) => "unchanged",
            };
            format!("{}{line} · {kind}", files[file].name())
        }
        None => String::new(),
    };
    let dot = move || match latest.get().map(|selection| selection.tone) {
        Some(Tone::Added) => GREEN,
        Some(Tone::Removed) => RED,
        _ => ACCENT,
    };
    div()
        .absolute()
        .bottom(24.0)
        .left(pct(50.0))
        .ml(-HUD_WIDTH / 2.0)
        .w(HUD_WIDTH)
        .h(HUD_HEIGHT)
        .flex()
        .items_center()
        .gap(10.0)
        .px(18.0)
        .rounded(HUD_HEIGHT / 2.0)
        .backdrop(HUD_GLASS)
        .opacity(move || if shown.get() { 1.0 } else { 0.0 })
        .translate_y(move || if shown.get() { 0.0 } else { 36.0 })
        .scale(move || if shown.get() { 1.0 } else { 0.85 })
        .transition(Spring::bouncy())
        .on_click(move |_| review.selected.set(None))
        .child(div().size(8.0).rounded(4.0).flex_none().bg(dot))
        .child(
            clipped(code(describe, TEXT))
                .font(CODE_FONT)
                .text_size(rem(0.75)),
        )
        .child(label("click to clear", 12.0, MUTED))
}

fn comment_editor(review: Review) -> Div {
    div()
        .test_id("diff-comment-editor")
        .absolute()
        .bottom(rem(1.5))
        .left(rem(1.0))
        .right(rem(1.5))
        .flex()
        .col()
        .gap(rem(0.5))
        .p(rem(0.75))
        .rounded(12.0)
        .bg(BASE)
        .border(1.0)
        .border_color(ACCENT)
        .child(label(
            "Comment · Enter to save, Shift-Enter for a newline",
            12.0,
            MUTED,
        ))
        .child(
            editable(review.draft)
                .test_id("diff-comment-draft")
                .autofocus()
                .multiline()
                .min_rows(2)
                .max_rows(5)
                .submit_on_enter(true)
                .on_submit(move |value| {
                    if let Some(selection) = review.selected.get_untracked()
                        && !value.trim().is_empty()
                    {
                        review.comments.update(|comments| {
                            if let Some((_, text)) =
                                comments.iter_mut().find(|(line, _)| *line == selection)
                            {
                                value.clone_into(text);
                            } else {
                                comments.push((selection, value.to_owned()));
                            }
                        });
                        review.commenting.set(false);
                    }
                }),
        )
        .child(
            div()
                .test_id("diff-comment-cancel")
                .on_click(move |_| review.commenting.set(false))
                .child(label("Cancel", 12.0, MUTED)),
        )
}

fn row(review: Review, index: usize) -> impl View {
    let current = memo(move || review.presentation.with(|p| p.row(index)));
    dynamic(move || {
        let Some(row) = current.get() else {
            return div();
        };
        match &row.kind {
            RowKind::Header => file_header(review, row.file),
            RowKind::Hunk(header) => hunk_header(header),
            RowKind::Unified(cell) => code_cell(review, &row, Side::New, Some(cell)),
            RowKind::Split(old, new) => div()
                .flex()
                .size_full()
                .child(code_cell(review, &row, Side::Old, old.as_ref()))
                .child(div().w(1.0).h_full().flex_none().bg(RIM))
                .child(code_cell(review, &row, Side::New, new.as_ref())),
        }
    })
}

fn file_header(review: Review, file: usize) -> Div {
    let bar = header_bar(review, file)
        .bg(LIFT)
        .hover(|style| style.bg(PRESS));
    div()
        .size_full()
        .px(8.0)
        .py(4.0)
        .opacity(move || {
            if review.active_file.get() == file {
                0.0
            } else {
                1.0
            }
        })
        .child(bar)
}

fn header_bar(review: Review, file: usize) -> Div {
    let Some(diff) = review.files().get(file).cloned() else {
        return div();
    };
    let collapsed = memo(move || review.collapsed.with(|collapsed| collapsed[file]));
    let chevron = div()
        .flex_none()
        .rotate(move || if collapsed.get() { 0.0 } else { 90.0 })
        .transition(Spring::snappy())
        .child(label("›", 16.0, MUTED));
    let directory = diff
        .directory()
        .map(|directory| label(format!("{directory}/"), 13.0, MUTED));
    let renamed = diff
        .renamed_from
        .as_ref()
        .map(|old| label(format!("← {old}"), 13.0, FAINT));
    div()
        .size_full()
        .flex()
        .items_center()
        .gap(8.0)
        .px(12.0)
        .rounded(6.0)
        .border(1.0)
        .border_color(RIM)
        .overflow_hidden()
        .on_click(move |_| review.toggle_file(file))
        .child(chevron)
        .child(status_code(diff.status))
        .child(label(diff.name(), 13.0, TEXT).weight(600.0))
        .child(clipped(
            div().flex().gap(8.0).children(directory).children(renamed),
        ))
        .child(stats(diff.added, diff.removed))
}

fn hunk_header(header: &str) -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .pl(INDICATOR_WIDTH + GUTTER_WIDTH)
        .overflow_hidden()
        .bg(HUNK)
        .font(CODE_FONT)
        .text_size(rem(0.75))
        .child(code(header, ACCENT))
}

fn code_cell(review: Review, row: &Row, side: Side, cell: Option<&Cell>) -> Div {
    let frame = div()
        .relative()
        .group("diff-line")
        .font(CODE_FONT)
        .text_size(rem(0.75))
        .flex()
        .items_center()
        .flex_1()
        .min_w(0.0)
        .h_full()
        .overflow_hidden();
    let Some(cell) = cell else {
        return frame.bg(Color::BLACK.with_alpha(0.18));
    };
    let colors = theme::tone(cell.tone);
    let selection = Selection {
        key: row.key,
        side,
        file: row.file,
        number: cell.number,
        tone: cell.tone,
    };
    let selected = memo(move || review.selected.get() == Some(selection));
    let number = cell
        .number
        .map(|number| code(number.to_string(), colors.gutter));
    let gutter = div()
        .flex()
        .flex_none()
        .justify_end()
        .w(GUTTER_WIDTH)
        .pr(12.0)
        .children(number);
    let content = cell.line.text.clone();
    let spans = cell
        .line
        .spans
        .iter()
        .map(|span| {
            (
                span.range.clone(),
                SpanStyle {
                    color: Some(theme::token(span.token)),
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();
    let id = format!("diff-line-{:?}-{side:?}", row.key);
    let comment = div()
        .test_id(format!("{id}-comment"))
        .absolute()
        .right(rem(0.25))
        .top(1.0)
        .size(20.0)
        .flex()
        .items_center()
        .justify_center()
        .rounded(4.0)
        .bg(Color::TRANSPARENT)
        .text_color(Color::TRANSPARENT)
        .group_hover("diff-line", |style| style.bg(ACCENT).text_color(BASE))
        .on_click(move |event| {
            event.stop_propagation();
            review.selected.set(Some(selection));
            review.draft.set(review.comments.with_untracked(|comments| {
                comments
                    .iter()
                    .find(|(line, _)| *line == selection)
                    .map_or_else(String::new, |(_, text)| text.clone())
            }));
            review.commenting.set(true);
        })
        .child(text("+"));
    frame
        .test_id(id.clone())
        .bg(move || {
            if selected.get() {
                SELECTION
            } else {
                colors.background
            }
        })
        .hover(move |style| style.bg(colors.hover))
        .on_click(move |_| review.selected.set(Some(selection)))
        .child(
            div()
                .w(INDICATOR_WIDTH)
                .h_full()
                .flex_none()
                .bg(colors.indicator),
        )
        .child(gutter)
        .child(
            div()
                .test_id(format!("{id}-text"))
                .flex_none()
                .child(rich_text(content, spans).no_wrap()),
        )
        .child(comment)
}

fn stats(added: usize, removed: usize) -> Div {
    let stat = |count: usize, sign: char, color: Color| {
        (count > 0).then(|| code(format!("{sign}{count}"), color))
    };
    div()
        .font(CODE_FONT)
        .text_size(rem(0.75))
        .flex()
        .flex_none()
        .gap(rem(0.375))
        .children(stat(added, '+', GREEN))
        .children(stat(removed, '−', RED))
}

fn status_code(status: Status) -> Text {
    code(status.code().to_string(), theme::status(status))
        .font(CODE_FONT)
        .text_size(rem(0.75))
}

fn clipped(content: impl View) -> Div {
    div().flex_1().min_w(0.0).overflow_hidden().child(content)
}

fn label<T>(content: impl IntoProp<String, T>, size: f32, color: Color) -> Text {
    text(content)
        .text_size(rem(size / 16.0))
        .color(color)
        .no_wrap()
        .flex_none()
}

fn code<T>(content: impl IntoProp<String, T>, color: Color) -> Text {
    text(content).color(color).no_wrap().flex_none()
}
