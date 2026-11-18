use rouge_reactive::{ReadSignal, expect_context};
use rouge_render::Size;

pub const COMPACT_WIDTH: f32 = 600.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputKind {
    #[default]
    Pointer,
    Touch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WidthClass {
    Compact,
    #[default]
    Regular,
}

impl WidthClass {
    pub fn of(size: Size) -> Self {
        if size.width < COMPACT_WIDTH {
            WidthClass::Compact
        } else {
            WidthClass::Regular
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Presentation {
    pub input: InputKind,
    pub width: WidthClass,
}

impl Presentation {
    pub fn is_touch(self) -> bool {
        self.input == InputKind::Touch
    }

    pub fn is_compact(self) -> bool {
        self.width == WidthClass::Compact
    }

    pub fn is_handheld(self) -> bool {
        self.is_touch() && self.is_compact()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Presented(pub(crate) ReadSignal<Presentation>);

pub fn presentation() -> ReadSignal<Presentation> {
    expect_context::<Presented>().0
}
