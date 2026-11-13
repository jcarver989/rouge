use crate::fonts::bundled_fonts;
use rouge_core::testing::TestUiBuilder;
use rouge_theme::UiPalette;

pub fn component_test() -> TestUiBuilder {
    TestUiBuilder::default()
        .fonts(bundled_fonts())
        .palette(UiPalette::default())
}
