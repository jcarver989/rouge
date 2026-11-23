use crate::tree::RowSource;
use crate::view::{Div, Text};
use rouge_reactive::Memo;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextBreak {
    None,
    Space,
    Tab,
    #[default]
    Line,
    Paragraph,
}

impl TextBreak {
    pub const fn as_str(self) -> &'static str {
        match self {
            TextBreak::None => "",
            TextBreak::Space => " ",
            TextBreak::Tab => "\t",
            TextBreak::Line => "\n",
            TextBreak::Paragraph => "\n\n",
        }
    }
}

impl Div {
    pub fn selectable(mut self) -> Self {
        self.spec.extras().selection = Some(Box::default());
        self.tab_index(-1)
    }

    pub fn unselectable(mut self) -> Self {
        self.spec.flags.unselectable = true;
        self
    }

    pub(crate) fn selection_row(mut self, row: Memo<usize>) -> Self {
        self.spec.extras().select_row = Some(row);
        self
    }

    pub(crate) fn row_source(mut self, source: RowSource) -> Self {
        self.spec.extras().row_source = Some(source);
        self
    }
}

impl Text {
    pub fn unselectable(self) -> Self {
        Text(self.0.unselectable())
    }

    pub fn selection_break(mut self, separator: TextBreak) -> Self {
        if let Some(text) = self.0.spec.kind.text_mut() {
            text.selection_break = separator;
        }
        self
    }
}
