use rouge_core::{Cursor, Ui};
use rouge_render::{Affine, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TextInput {
    Off,
    On { area: Option<Rect> },
}

#[derive(Debug, Default)]
pub(crate) struct Changes {
    pub(crate) cursor: Option<Cursor>,
    pub(crate) text_input: Option<TextInput>,
}

#[derive(Debug, Default)]
pub(crate) struct Tracked {
    cursor: Option<Cursor>,
    text_input: Option<TextInput>,
}

impl Tracked {
    pub(crate) fn changes(&mut self, ui: &Ui) -> Changes {
        let mut changes = Changes::default();
        let cursor = ui.cursor();
        if self.cursor != Some(cursor) {
            self.cursor = Some(cursor);
            changes.cursor = Some(cursor);
        }
        let text_input = if ui.accepts_text() {
            let scale = ui.scale();
            let area = ui.text_cursor_area();
            TextInput::On {
                area: area.map(|area| Affine::scale(scale, scale).apply_rect(&area)),
            }
        } else {
            TextInput::Off
        };
        if self.text_input != Some(text_input) {
            self.text_input = Some(text_input);
            changes.text_input = Some(text_input);
        }
        changes
    }
}
