use rouge_theme::{
    ButtonVariant, ControlState, InteractionState, NoticeTone, Rgba, SelectionState, UiPalette,
};

#[test]
fn ui_palette_uses_stable_semantic_roles() {
    let ui = UiPalette::default();
    assert_ne!(ui.surface, ui.surface_selected);
    assert_ne!(ui.text_secondary, ui.text_muted);
    assert_ne!(ui.info, ui.text_muted);
    assert_ne!(ui.warning, ui.accent);
    assert_ne!(ui.info, ui.accent);
    assert_eq!(UiPalette::default(), UiPalette::for_dark(false));
    assert_eq!(UiPalette::dark(), UiPalette::for_dark(true));
}

#[test]
fn semantic_states_resolve_before_renderer_conversion() {
    let ui = UiPalette::default();
    let primary = ui.control_style(
        ButtonVariant::Primary,
        ControlState::new(InteractionState::Rest),
    );
    assert_eq!(
        primary,
        ui.control_style(
            ButtonVariant::Primary,
            ControlState::new(InteractionState::Hovered)
        )
    );
    assert_eq!(primary.background, Some(ui.accent));
    assert_eq!(primary.foreground, ui.accent_foreground);

    let selected_hover = ui.control_style(
        ButtonVariant::Ghost,
        ControlState::new(InteractionState::Hovered).selected(true),
    );
    assert_eq!(selected_hover.background, Some(ui.surface_selected));
    assert_eq!(selected_hover.foreground, ui.accent);

    let focused = ui.selection_style(SelectionState::Focused);
    assert_eq!(focused.background, Some(ui.accent));
    assert!(focused.emphasized);

    let warning = ui.notice_style(NoticeTone::Warning);
    assert_eq!(warning.foreground, ui.warning);
    assert!(warning.emphasized);
    assert_eq!(
        ui.notice_style(NoticeTone::Neutral).foreground,
        ui.text_secondary
    );
}

#[test]
fn accent_foreground_reads_on_the_accent_in_both_appearances() {
    for ui in [UiPalette::light(), UiPalette::dark()] {
        let ratio = ui.accent_foreground.contrast(ui.accent);
        assert!(ratio >= 4.5, "contrast {ratio} is below 4.5");
    }
}

#[test]
fn controls_are_flat_fills_or_hairline_outlines() {
    let ui = UiPalette::default();
    let rest = |variant| ui.control_style(variant, ControlState::new(InteractionState::Rest));
    let hovered = |variant| ui.control_style(variant, ControlState::new(InteractionState::Hovered));
    assert_eq!(rest(ButtonVariant::Secondary).background, Some(ui.elevated));
    assert_eq!(rest(ButtonVariant::Ghost).foreground, ui.text_secondary);
    assert_eq!(rest(ButtonVariant::Destructive).background, None);
    assert_eq!(rest(ButtonVariant::Destructive).foreground, ui.destructive);
    assert_eq!(
        hovered(ButtonVariant::Destructive).background,
        Some(ui.surface_hover)
    );
    for variant in [
        ButtonVariant::Primary,
        ButtonVariant::Secondary,
        ButtonVariant::Destructive,
        ButtonVariant::Ghost,
    ] {
        assert_eq!(
            ui.control_style(variant, ControlState::new(InteractionState::Disabled)),
            rest(variant),
            "disabled controls fade rather than recolour"
        );
    }
}

#[test]
fn the_dark_palette_layers_chrome_canvas_and_elevated_surfaces() {
    let ui = UiPalette::dark();
    let lightness = |color: Rgba| u32::from(color.r) + u32::from(color.g) + u32::from(color.b);
    assert!(lightness(ui.chrome) < lightness(ui.canvas));
    assert!(lightness(ui.canvas) < lightness(ui.elevated));
    assert!(lightness(ui.border_subtle) < lightness(ui.border));
}
