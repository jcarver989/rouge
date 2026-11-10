use super::patch::{FileDiff, Hunk, Patch, Tone};
use super::syntax::{HighlightedLine, Language, highlight};
use std::iter::repeat_n;
use std::mem::take;
use std::ops::Range;
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
    pub number: Option<usize>,
    pub line: Rc<HighlightedLine>,
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
            let (mut unified, mut split) = (Vec::new(), Vec::new());
            for hunk in &diff.hunks {
                let (hunk_unified, hunk_split) = hunk_rows(hunk, language);
                unified.extend(hunk_unified);
                split.extend(hunk_split);
            }
            let row = move |key: RowKey, kind: RowKind| Rc::new(Row { key, file, kind });
            FileRows {
                header: row(RowKey::Header(file), RowKind::Header),
                unified: zip(
                    unified.into_iter(),
                    |index| RowKey::Unified(file, index),
                    row,
                ),
                split: zip(split.into_iter(), |index| RowKey::Split(file, index), row),
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
            presentation.file_rows.push(presentation.rows.len());
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
    file_rows: Vec<usize>,
}

impl Presentation {
    fn push(&mut self, row: &Rc<Row>) {
        self.rows.push(row.clone());
    }

    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(super) fn row(&self, index: usize) -> Option<Rc<Row>> {
        self.rows.get(index).cloned()
    }

    pub(super) fn file_rows(&self, file: usize) -> Range<usize> {
        let start = self.file_rows.get(file).copied().unwrap_or(self.len());
        let end = self.file_rows.get(file + 1).copied().unwrap_or(self.len());
        start..end
    }

    pub(super) fn file_at(&self, offset: f32, top: impl Fn(usize) -> f32) -> usize {
        self.file_rows
            .partition_point(|row| top(*row) <= offset)
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

fn hunk_rows(hunk: &Hunk, language: Language) -> (Vec<RowKind>, Vec<RowKind>) {
    let mut unified = vec![RowKind::Hunk(hunk.header.clone())];
    let mut split = vec![RowKind::Hunk(hunk.header.clone())];
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    for line in &hunk.lines {
        let cell = Cell {
            tone: line.kind,
            number: line.new_line_no.or(line.old_line_no),
            line: highlight(&line.text, language),
        };
        unified.push(RowKind::Unified(cell.clone()));
        match line.kind {
            Tone::HunkHeader | Tone::Meta => {
                split.extend(pair(take(&mut removed), take(&mut added)));
                split.push(RowKind::Hunk(line.text.to_string()));
            }
            Tone::Removed => removed.push(cell),
            Tone::Added => added.push(cell),
            Tone::Context => {
                split.extend(pair(take(&mut removed), take(&mut added)));
                let old = Cell {
                    number: line.old_line_no,
                    ..cell.clone()
                };
                split.push(RowKind::Split(Some(old), Some(cell)));
            }
        }
    }
    split.extend(pair(removed, added));
    (unified, split)
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
