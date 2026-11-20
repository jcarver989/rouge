#![allow(dead_code)]

use rouge_core::testing::TestUiBuilder;
use rouge_core::{Color, InputKind};
use rouge_widgets::testing::component_test;

pub(crate) fn builder() -> TestUiBuilder {
    TestUiBuilder::default()
        .size(300.0, 200.0)
        .clear(Color::WHITE)
}

pub(crate) fn handheld_test() -> TestUiBuilder {
    component_test().size(390.0, 700.0).input(InputKind::Touch)
}

pub(crate) fn line_widths(pixels: impl IntoIterator<Item = [u8; 4]>, color: [u8; 4]) -> Vec<usize> {
    let mut widths = Vec::new();
    let mut width = 0;
    for pixel in pixels {
        if pixel.iter().zip(color).all(|(a, b)| a.abs_diff(b) <= 2) {
            width += 1;
        } else if width > 0 {
            widths.push(width);
            width = 0;
        }
    }
    widths
}
