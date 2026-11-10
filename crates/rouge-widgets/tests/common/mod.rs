use rouge_core::Color;
use rouge_core::testing::TestUiBuilder;

pub(crate) fn builder() -> TestUiBuilder {
    TestUiBuilder::default()
        .size(300.0, 200.0)
        .clear(Color::WHITE)
}
