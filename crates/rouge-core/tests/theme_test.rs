use rouge_core::testing::{TestResult, TestUiBuilder, assert_near};
use rouge_core::{Color, Palette, Styled, div, provide_theme, use_theme};
use rouge_reactive::Runtime;

#[test]
fn a_default_theme_is_provided_and_switching_it_repaints_bound_props_only() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, theme) = TestUiBuilder::default().build_with(|| {
        let theme = use_theme();
        let root = div().size_full().bg(theme.surface).rounded(theme.radius);
        (root, theme)
    });
    assert_eq!(theme.palette(), Palette::light());
    assert_near(ui.pixel(100, 50)?, [255, 255, 255, 255], "light surface");
    let (layouts, _) = ui.passes();
    theme.set(Palette {
        surface: Color::RED,
        ..Palette::dark()
    });
    ui.advance(0.0);
    assert_near(ui.pixel(100, 50)?, [255, 0, 0, 255], "new surface colour");
    assert_eq!(ui.passes().0, layouts, "a theme switch needs no layout");
    assert_eq!(theme.text.get_untracked(), Palette::dark().text);
    Ok(())
}

#[test]
fn a_subtree_can_provide_its_own_theme() {
    let _rt = Runtime::new().enter();
    let (_ui, seen) = TestUiBuilder::default().build_with(|| {
        provide_theme(Palette::dark());
        (div(), use_theme().palette())
    });
    assert_eq!(seen, Palette::dark());
}
