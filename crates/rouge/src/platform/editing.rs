use rouge_core::{Editor, EditorId, InputHints};
use std::mem::replace;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Editing {
    pub(crate) id: EditorId,
    pub(crate) text: String,
    pub(crate) selection: Range<usize>,
    pub(crate) marked: Option<Range<usize>>,
    pub(crate) multiline: bool,
    pub(crate) hints: InputHints,
}

pub(crate) struct Previous(pub(crate) Option<Editing>);

impl Editing {
    pub(crate) fn track(
        known: &mut Option<Editing>,
        editor: Option<&Editor<'_>>,
    ) -> Option<Previous> {
        let same = match (&*known, editor) {
            (Some(known), Some(editor)) => known.matches(editor),
            (known, editor) => known.is_none() && editor.is_none(),
        };
        (!same).then(|| Previous(replace(known, editor.map(Editing::read))))
    }

    pub(crate) fn holds(&self, editor: &Editor<'_>) -> bool {
        self.id == editor.id() && self.text == editor.text()
    }

    fn matches(&self, editor: &Editor<'_>) -> bool {
        self.holds(editor)
            && self.selection == selected(editor)
            && self.marked == editor.marked()
            && self.multiline == editor.multiline()
            && self.hints == editor.hints()
    }

    fn read(editor: &Editor<'_>) -> Self {
        Editing {
            id: editor.id(),
            text: editor.text().to_owned(),
            selection: selected(editor),
            marked: editor.marked(),
            multiline: editor.multiline(),
            hints: editor.hints(),
        }
    }
}

pub(crate) fn selected(editor: &Editor<'_>) -> Range<usize> {
    let (anchor, focus) = editor.selection();
    anchor.min(focus)..anchor.max(focus)
}
