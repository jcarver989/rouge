use rouge_core::testing::{TestResult, TestUiBuilder, assert_near};
use rouge_core::{Color, Fill, Palette, Role, Styled, Theme, div, text};
use rouge_reactive::Runtime;
use rouge_theme::{
    ButtonVariant, ControlState, InteractionState, NoticeTone, Rgba, SelectionState, UiPalette,
};

#[test]
fn a_default_theme_is_provided_and_switching_it_repaints_bound_props_only() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, theme) = TestUiBuilder::default().build_with(|| {
        let theme = Theme::current();
        let root = div().size_full().bg(Role::Elevated).rounded(8.0);
        (root, theme)
    });
    assert_eq!(theme.get_untracked(), UiPalette::light().map(Color::from));
    assert_near(ui.pixel(100, 50)?, [255, 255, 255, 255], "light surface");
    let (layouts, _) = ui.passes();
    theme.set(Palette {
        elevated: Color::RED,
        ..UiPalette::dark().map(Color::from)
    });
    ui.advance(0.0);
    assert_near(ui.pixel(100, 50)?, [255, 0, 0, 255], "new surface colour");
    assert_eq!(ui.passes().0, layouts, "a theme switch needs no layout");
    assert_eq!(theme.get_untracked().text, UiPalette::dark().text.into());
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
    let primary = Theme::control_style(
        ButtonVariant::Primary,
        ControlState::new(InteractionState::Rest),
    );
    assert_eq!(
        primary.background.map(|fill| theme.resolve(fill)),
        Some(Color::from(ui.accent))
    );
    assert_eq!(
        theme.resolve(primary.foreground),
        Color::from(ui.accent_foreground)
    );
    assert_eq!(theme.resolve(primary.foreground).to_rgba8()[3], 96);
    assert_eq!(
        theme.resolve(Theme::notice_style(NoticeTone::Info).foreground),
        Color::from(ui.info)
    );
    assert_eq!(
        Theme::selection_style(SelectionState::Focused).foreground,
        Fill::Role(Role::AccentForeground)
    );
    assert_eq!(theme.get_untracked().canvas, Color::from(ui.canvas));
}

#[test]
fn a_theme_switch_runs_no_effects_and_repaints_roles_hover_and_inherited_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, theme) = TestUiBuilder::default().size(200.0, 100.0).build_with(|| {
        let theme = Theme::current();
        let cell = |index: usize| {
            div()
                .test_id(format!("cell-{index}"))
                .size(10.0)
                .bg(Role::Surface)
                .border(1.0)
                .border_color(Role::Border)
                .hover(|style| style.bg(Role::Accent))
                .child(text("x"))
        };
        let root = div()
            .size_full()
            .flex()
            .flex_wrap()
            .text_color(Role::TextMuted)
            .children((0..200).map(cell));
        (root, theme)
    });
    ui.hover_on("cell-0")?;
    let before = ui.stats();
    theme.set(Palette {
        accent: Color::GREEN,
        ..UiPalette::dark().map(Color::from)
    });
    ui.advance(0.0);
    let work = ui.stats() - before;
    assert_eq!(work.reactive.effects_run, 0, "roles resolve at paint");
    assert_eq!((work.layouts, work.paints), (0, 1));
    assert_near(
        ui.pixel(5, 5)?,
        [0, 255, 0, 255],
        "the hovered cell takes the new accent",
    );
    let surface = Color::from(UiPalette::dark().surface).to_rgba8();
    assert_near(ui.pixel(25, 5)?, surface, "others the new surface");
    Ok(())
}

#[test]
fn a_layered_element_takes_its_role_background_from_the_theme_it_was_built_under() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().size(200.0, 100.0).build(|| {
        Theme::provide(UiPalette::dark());
        div()
            .size_full()
            .child(div().layer().size_full().bg(Role::Canvas))
    });
    let canvas = Color::from(UiPalette::dark().canvas).to_rgba8();
    assert_near(
        ui.pixel(100, 50)?,
        canvas,
        "the dark canvas, not the default light one",
    );
    Ok(())
}

#[test]
fn a_subtree_can_provide_its_own_theme() {
    let _rt = Runtime::new().enter();
    let (_ui, seen) = TestUiBuilder::default().build_with(|| {
        Theme::provide(UiPalette::dark());
        (div(), Theme::current().get_untracked())
    });
    assert_eq!(seen, UiPalette::dark().map(Color::from));
}
