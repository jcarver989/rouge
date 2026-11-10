use super::patch::{FileDiff, Hunk, Line, Patch, Tone};
use super::syntax::{Language, Span, highlight};
use std::iter::repeat_n;
use std::mem::take;
use std::rc::Rc;

pub(super) const ROW_HEIGHT: f32 = 22.0;
pub(super) const HEADER_HEIGHT: f32 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum RowKey {
    Header(usize),
    Unified(usize, usize),
    Split(usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Cell {
    pub tone: Tone,
    pub number: Option<u32>,
    pub spans: Rc<[Span]>,
}

#[derive(Debug, PartialEq)]
pub(super) enum RowKind {
    Header,
    Hunk(String),
    Unified(Cell),
    Split(Option<Cell>, Option<Cell>),
}

#[derive(Debug, PartialEq)]
pub(super) struct Row {
    pub key: RowKey,
    pub file: usize,
    pub kind: RowKind,
}

impl Row {
    pub(super) fn height(&self) -> f32 {
        if self.kind == RowKind::Header {
            HEADER_HEIGHT
        } else {
            ROW_HEIGHT
        }
    }
}

struct FileRows {
    diff: Rc<FileDiff>,
    header: Rc<Row>,
    unified: Vec<Rc<Row>>,
    split: Vec<Rc<Row>>,
}

pub(super) struct Document {
    files: Vec<FileRows>,
}

impl Document {
    pub(super) fn new(patch: Patch) -> Document {
        let files = patch.files.into_iter().enumerate().map(|(file, diff)| {
            let language = Language::from_path(&diff.path);
            let kinds = |split: bool| {
                diff.hunks
                    .iter()
                    .flat_map(move |hunk| hunk_rows(hunk, language, split))
            };
            let row = move |key: RowKey, kind: RowKind| Rc::new(Row { key, file, kind });
            FileRows {
                header: row(RowKey::Header(file), RowKind::Header),
                unified: zip(kinds(false), |index| RowKey::Unified(file, index), row),
                split: zip(kinds(true), |index| RowKey::Split(file, index), row),
                diff: Rc::new(diff),
            }
        });
        Document {
            files: files.collect(),
        }
    }

    pub(super) fn files(&self) -> Vec<Rc<FileDiff>> {
        self.files.iter().map(|file| file.diff.clone()).collect()
    }

    pub(super) fn present(&self, split: bool, collapsed: &[bool]) -> Presentation {
        let mut presentation = Presentation::default();
        for (index, file) in self.files.iter().enumerate() {
            presentation.file_tops.push(presentation.height);
            presentation.push(&file.header);
            let rows = if split { &file.split } else { &file.unified };
            if !collapsed.get(index).copied().unwrap_or_default() {
                for row in rows {
                    presentation.push(row);
                }
            }
        }
        presentation
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct Presentation {
    rows: Vec<Rc<Row>>,
    file_tops: Vec<f32>,
    height: f32,
}

impl Presentation {
    fn push(&mut self, row: &Rc<Row>) {
        self.rows.push(row.clone());
        self.height += row.height();
    }

    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(super) fn row(&self, index: usize) -> Option<Rc<Row>> {
        self.rows.get(index).cloned()
    }

    pub(super) fn file_span(&self, file: usize) -> (f32, f32) {
        let top = self.file_tops.get(file).copied().unwrap_or(self.height);
        (
            top,
            self.file_tops.get(file + 1).copied().unwrap_or(self.height),
        )
    }

    pub(super) fn file_at(&self, offset: f32) -> usize {
        self.file_tops
            .partition_point(|top| *top <= offset)
            .saturating_sub(1)
    }
}

fn zip<T>(
    kinds: impl Iterator<Item = RowKind>,
    key: impl Fn(usize) -> RowKey,
    row: impl Fn(RowKey, RowKind) -> T,
) -> Vec<T> {
    kinds
        .enumerate()
        .map(|(index, kind)| row(key(index), kind))
        .collect()
}

fn hunk_rows(hunk: &Hunk, language: Language, split: bool) -> Vec<RowKind> {
    let cell = |line: &Line, number: Option<u32>| Cell {
        tone: line.tone,
        number,
        spans: highlight(&line.text.replace('\t', "    "), language),
    };
    let mut rows = vec![RowKind::Hunk(hunk.header.clone())];
    if !split {
        let unified = |line| RowKind::Unified(cell(line, line.new.or(line.old)));
        rows.extend(hunk.lines.iter().map(unified));
        return rows;
    }
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    for line in &hunk.lines {
        match line.tone {
            Tone::Removed => removed.push(cell(line, line.old)),
            Tone::Added => added.push(cell(line, line.new)),
            Tone::Context => {
                rows.extend(pair(take(&mut removed), take(&mut added)));
                rows.push(RowKind::Split(
                    Some(cell(line, line.old)),
                    Some(cell(line, line.new)),
                ));
            }
        }
    }
    rows.extend(pair(removed, added));
    rows
}

fn pair(removed: Vec<Cell>, added: Vec<Cell>) -> impl Iterator<Item = RowKind> {
    let len = removed.len().max(added.len());
    let side = move |cells: Vec<Cell>| {
        let padding = len - cells.len();
        cells.into_iter().map(Some).chain(repeat_n(None, padding))
    };
    side(removed)
        .zip(side(added))
        .map(|(old, new)| RowKind::Split(old, new))
}
