use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Modified,
    Added,
    Deleted,
    Renamed,
}

impl Status {
    pub(crate) const fn code(self) -> char {
        match self {
            Self::Modified => 'M',
            Self::Added => 'A',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Line {
    pub kind: Tone,
    pub text: Arc<str>,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
    pub no_newline: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Hunk {
    pub header: String,
    pub function_context: Option<String>,
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FileDiff {
    pub path: String,
    pub renamed_from: Option<String>,
    pub status: Status,
    pub hunks: Vec<Hunk>,
    pub added: usize,
    pub removed: usize,
}

impl FileDiff {
    fn new(path: &str) -> Self {
        FileDiff {
            path: path.to_owned(),
            renamed_from: None,
            status: Status::Modified,
            hunks: Vec::new(),
            added: 0,
            removed: 0,
        }
    }

    pub(crate) fn name(&self) -> &str {
        self.path
            .rsplit_once('/')
            .map_or(&self.path, |(_, name)| name)
    }

    pub(crate) fn directory(&self) -> Option<&str> {
        self.path.rsplit_once('/').map(|(directory, _)| directory)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Patch {
    pub files: Vec<FileDiff>,
}

impl Patch {
    pub(crate) fn parse(source: &str) -> Patch {
        let mut files: Vec<FileDiff> = Vec::new();
        let (mut old, mut new) = (Remaining::default(), Remaining::default());
        for line in source.lines() {
            let in_hunk = old.left > 0 || new.left > 0;
            let file = match files.last_mut() {
                Some(file) if in_hunk => {
                    file.push_line(line, &mut old, &mut new);
                    continue;
                }
                Some(file) if !starts_file(line, file) => file,
                _ => match file_path(line) {
                    Some(path) => {
                        files.push(FileDiff::new(path));
                        continue;
                    }
                    None => continue,
                },
            };
            if let Some(ranges) = hunk_ranges(line) {
                (old, new) = ranges;
                file.hunks.push(Hunk {
                    header: line.to_owned(),
                    function_context: line
                        .split("@@")
                        .nth(2)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned),
                    old_start: old.number,
                    old_count: old.left,
                    new_start: new.number,
                    new_count: new.left,
                    lines: Vec::new(),
                });
            } else if line.starts_with("\\ No newline") {
                file.push_line(line, &mut old, &mut new);
            } else if line.starts_with("new file mode") || line == "--- /dev/null" {
                file.status = Status::Added;
            } else if line.starts_with("deleted file mode") || line == "+++ /dev/null" {
                file.status = Status::Deleted;
            } else if let Some(from) = line.strip_prefix("rename from ") {
                file.renamed_from = Some(from.to_owned());
                file.status = Status::Renamed;
            } else if let Some(to) = line.strip_prefix("rename to ") {
                to.clone_into(&mut file.path);
            } else if let Some(path) = line.strip_prefix("+++ ") {
                strip_side(path).clone_into(&mut file.path);
            }
        }
        Patch { files }
    }
}

impl FileDiff {
    fn push_line(&mut self, line: &str, old: &mut Remaining, new: &mut Remaining) {
        let Some(hunk) = self.hunks.last_mut() else {
            return;
        };
        let (tone, text) = match line.split_at_checked(1) {
            Some(("+", text)) => (Tone::Added, text),
            Some(("-", text)) => (Tone::Removed, text),
            Some(("\\", _)) => {
                if let Some(line) = hunk.lines.last_mut() {
                    line.no_newline = true;
                }
                return;
            }
            Some((_, text)) => (Tone::Context, text),
            None => (Tone::Context, ""),
        };
        let (old, new) = match tone {
            Tone::Context => (old.next(), new.next()),
            Tone::Added => (None, new.next()),
            Tone::Removed => (old.next(), None),
        };
        self.added += usize::from(tone == Tone::Added);
        self.removed += usize::from(tone == Tone::Removed);
        hunk.lines.push(Line {
            kind: tone,
            old_line_no: old,
            new_line_no: new,
            text: text.into(),
            no_newline: false,
        });
    }
}

#[derive(Clone, Copy, Default)]
struct Remaining {
    number: usize,
    left: usize,
}

impl Remaining {
    fn next(&mut self) -> Option<usize> {
        let number = self.number;
        self.number += 1;
        self.left = self.left.saturating_sub(1);
        Some(number)
    }
}

fn starts_file(line: &str, current: &FileDiff) -> bool {
    line.starts_with("diff --git ") || (line.starts_with("--- ") && !current.hunks.is_empty())
}

fn file_path(line: &str) -> Option<&str> {
    match line.strip_prefix("diff --git ") {
        Some(paths) => paths.rsplit_once(" b/").map(|(_, path)| path),
        None => line.strip_prefix("--- ").map(strip_side),
    }
}

fn strip_side(path: &str) -> &str {
    let path = path.split('\t').next().unwrap_or(path);
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
}

fn hunk_ranges(header: &str) -> Option<(Remaining, Remaining)> {
    let mut parts = header.strip_prefix("@@ -")?.split(' ');
    let old = range(parts.next()?)?;
    let new = range(parts.next()?.strip_prefix('+')?)?;
    Some((old, new))
}

fn range(text: &str) -> Option<Remaining> {
    let (start, len) = text.split_once(',').unwrap_or((text, "1"));
    Some(Remaining {
        number: start.parse().ok()?,
        left: len.parse().ok()?,
    })
}
