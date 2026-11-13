use rouge_core::testing::{TestResult, TestUiBuilder, assert_near};
use rouge_core::{Color, Palette, Styled, Theme, div};
use rouge_reactive::Runtime;
use rouge_theme::{
    ButtonVariant, ControlState, InteractionState, NoticeTone, Rgba, SelectionState, UiPalette,
};

#[test]
fn a_default_theme_is_provided_and_switching_it_repaints_bound_props_only() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, theme) = TestUiBuilder::default().build_with(|| {
        let theme = Theme::current();
        let root = div().size_full().bg(theme.elevated).rounded(8.0);
        (root, theme)
    });
    assert_eq!(theme.palette(), UiPalette::light().map(Color::from));
    assert_near(ui.pixel(100, 50)?, [255, 255, 255, 255], "light surface");
    let (layouts, _) = ui.passes();
    theme.set(Palette {
        elevated: Color::RED,
        ..UiPalette::dark().map(Color::from)
    });
    ui.advance(0.0);
    assert_near(ui.pixel(100, 50)?, [255, 0, 0, 255], "new surface colour");
    assert_eq!(ui.passes().0, layouts, "a theme switch needs no layout");
    assert_eq!(theme.text.get_untracked(), UiPalette::dark().text.into());
    Ok(())
}

#[test]
fn a_shared_ui_palette_resolves_component_styles_with_its_alpha() {
    let _rt = Runtime::new().enter();
    let ui = UiPalette {
        accent: Rgba::new(220, 140, 100, 128),
        accent_foreground: Rgba::new(30, 40, 60, 96),
        info: Rgba::new(30, 180, 240, 64),
        ..UiPalette::default()
    };
    let theme = Theme::new(ui);
    let primary = theme.control_style(
        ButtonVariant::Primary,
        ControlState::new(InteractionState::Rest),
    );
    assert_eq!(primary.background, Some(Color::from(ui.accent)));
    assert_eq!(primary.foreground, Color::from(ui.accent_foreground));
    assert_eq!(primary.foreground.to_rgba8()[3], 96);
    assert_eq!(
        theme.notice_style(NoticeTone::Info).foreground,
        Color::from(ui.info)
    );
    assert_eq!(
        theme.selection_style(SelectionState::Focused).foreground,
        Color::from(ui.accent_foreground)
    );
    assert_eq!(theme.canvas.get_untracked(), Color::from(ui.canvas));
}

#[test]
fn a_subtree_can_provide_its_own_theme() {
    let _rt = Runtime::new().enter();
    let (_ui, seen) = TestUiBuilder::default().build_with(|| {
        Theme::provide(UiPalette::dark());
        (div(), Theme::current().palette())
    });
    assert_eq!(seen, UiPalette::dark().map(Color::from));
}
