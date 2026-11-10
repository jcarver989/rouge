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
    number: Option<u32>,
    tone: Tone,
}

#[derive(Clone, Copy)]
struct Review {
    document: ReadSignal<Rc<Document>>,
    split: Signal<bool>,
    collapsed: Signal<Vec<bool>>,
    selected: Signal<Option<Selection>>,
    scroll: Signal<ScrollState>,
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
        let scroll = signal(ScrollState::default());

        let presentation =
            memo(move || Rc::new(document.with(|d| collapsed.with(|c| d.present(split.get(), c)))));
        let offset = memo(move || scroll.with(|state| state.offset.y.round()));
        let active_file = memo(move || presentation.with(|p| p.file_at(offset.get())));
        Review {
            document,
            split,
            collapsed,
            selected: signal(None),
            scroll,
            rows: signal(0..0),
            presentation,
            offset,
            active_file,
        }
    }

    fn files(self) -> Vec<Rc<FileDiff>> {
        self.document.with_untracked(|document| document.files())
    }

    fn file_span(self, file: usize) -> (f32, f32) {
        self.presentation
            .with_untracked(|presentation| presentation.file_span(file))
    }

    fn scroll_to(self, offset: f32) {
        self.scroll.update(|state| state.offset.y = offset);
    }

    fn scroll_to_file(self, file: usize) {
        self.scroll_to(self.file_span(file).0);
    }

    fn toggle_file(self, file: usize) {
        let top = self.file_span(file).0;
        self.collapsed
            .update(|collapsed| collapsed[file] = !collapsed[file]);
        if top < self.offset.get_untracked() {
            self.scroll_to(top);
        }
    }

    fn set_split(self, split: bool) {
        if self.split.get_untracked() == split {
            return;
        }
        let (file, offset) = (
            self.active_file.get_untracked(),
            self.offset.get_untracked(),
        );
        let (top, bottom) = self.file_span(file);
        let progress = (offset - top) / (bottom - top).max(1.0);
        self.split.set(split);
        let (top, bottom) = self.file_span(file);
        self.scroll_to(top + progress * (bottom - top));
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
            rows.start.max(1),
            rows.end.saturating_sub(1),
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
        .child(code(in_view, MUTED))
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
    let height = move |index: usize| match index.checked_sub(1) {
        Some(index) => presentation.with(|p| p.row(index).map_or(0.0, |row| row.height())),
        None => CHROME,
    };
    let rows = virtual_list(
        move || presentation.with(|p| p.len()) + 1,
        height,
        move |index| row(review, index),
    )
    .track_rows(review.rows);
    let rows = scroll_view(rows)
        .scroll_state(review.scroll)
        .absolute()
        .inset(0.0);
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
}

fn sticky_header(review: Review) -> Div {
    let push = memo(move || {
        let (_, bottom) = review
            .presentation
            .with(|p| p.file_span(review.active_file.get()));
        (bottom - review.offset.get() - HEADER_HEIGHT).min(0.0)
    });
    let bar = move || {
        header_bar(review, review.active_file.get())
            .border_color(RIM)
            .hover(|style| style.bg(LIFT))
            .backdrop(HEADER_GLASS)
    };
    div()
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
    let shown = memo(move || review.selected.with(Option::is_some));
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
                (Tone::Context, Side::Old) => "old side",
                (Tone::Context, Side::New) => "unchanged",
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
        .child(clipped(code(describe, TEXT)))
        .child(label("click to clear", 12.0, MUTED))
}

fn row(review: Review, index: usize) -> impl View {
    let current = memo(move || {
        let index = index.checked_sub(1)?;
        review.presentation.with(|p| p.row(index))
    });
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
        .child(code(header, ACCENT))
}

fn code_cell(review: Review, row: &Row, side: Side, cell: Option<&Cell>) -> Div {
    let frame = div()
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
    let spans = cell
        .spans
        .iter()
        .map(|span| code(span.text.as_str(), theme::token(span.token)));
    frame
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
        .children(spans)
}

fn stats(added: usize, removed: usize) -> Div {
    let stat = |count: usize, sign: char, color: Color| {
        (count > 0).then(|| code(format!("{sign}{count}"), color))
    };
    div()
        .flex()
        .flex_none()
        .gap(6.0)
        .children(stat(added, '+', GREEN))
        .children(stat(removed, '−', RED))
}

fn status_code(status: Status) -> Text {
    code(status.code(), theme::status(status))
}

fn clipped(content: impl View) -> Div {
    div().flex_1().min_w(0.0).overflow_hidden().child(content)
}

fn label<T>(content: impl IntoProp<String, T>, size: f32, color: Color) -> Text {
    text(content)
        .font(UI_FONT)
        .text_size(size)
        .color(color)
        .no_wrap()
        .flex_none()
}

fn code<T>(content: impl IntoProp<String, T>, color: Color) -> Text {
    text(content)
        .font(CODE_FONT)
        .text_size(12.0)
        .color(color)
        .no_wrap()
        .flex_none()
}
