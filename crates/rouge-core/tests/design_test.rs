use rouge_core::testing::{TestResult, TestUiBuilder, assert_near};
use rouge_core::{
    Apply, Color, Interactions, Key, Length, Styled, Typography, div, px, root_font_size,
    set_design_unit, styles, text, theme, virtual_list,
};
use rouge_reactive::{Runtime, signal};
use std::cell::Cell;
use std::rc::Rc;

theme! {
    pub struct Brand from BrandPalette {
        ink: Color,
        gutter: f32,
    }
}

styles! {
    trait Shorthands {
        wide(target) => target.w(60.0);
        inked(target) => target.bg(Brand::current().ink);
    }
}

#[test]
fn bare_numbers_scale_with_the_root_size_once_a_design_unit_is_set() -> TestResult {
    let _rt = Runtime::new().enter();
    set_design_unit(Some(16.0));
    assert_eq!(Length::design(8.0), Length::Rem(0.5));
    let (mut ui, root) = TestUiBuilder::default().build_with(|| {
        let view = div()
            .size_full()
            .child(div().w(32.0).h(px(10.0)).rounded(8.0).bg(Color::RED))
            .child(virtual_list(|| 3, 10.0, |_| div().w_full().h_full()).size_full());
        (view, root_font_size())
    });
    assert_eq!(ui.bounds(1.0, 5.0)?.size.width, 32.0);
    assert_eq!(ui.bounds(1.0, 11.0)?.size.height, 10.0);
    root.set(32.0);
    ui.run(0.1);
    let block = ui.bounds(1.0, 5.0)?;
    assert_eq!((block.size.width, block.size.height), (64.0, 10.0));
    assert_eq!(
        ui.bounds(1.0, 11.0)?.size.height,
        20.0,
        "rows follow the root size"
    );
    assert_near(
        ui.pixel(0, 0)?,
        [0, 0, 0, 0],
        "the corner radius doubled too",
    );
    set_design_unit(None);
    assert_eq!(Length::design(8.0), Length::Px(8.0));
    Ok(())
}

#[test]
fn applications_declare_their_own_theme_tokens_and_style_shorthands() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, theme) = TestUiBuilder::default().build_with(|| {
        let theme = Brand::provide(BrandPalette {
            ink: Color::RED,
            gutter: 4.0,
        });
        (div().h(10.0).wide().inked().ml(theme.gutter), theme)
    });
    assert_eq!(ui.bounds(5.0, 5.0)?.origin.x, 4.0);
    assert_eq!(ui.bounds(5.0, 5.0)?.size.width, 60.0);
    theme.set(BrandPalette {
        ink: Color::WHITE,
        gutter: 8.0,
    });
    ui.advance(0.0);
    assert_near(ui.pixel(20, 5)?, [255, 255, 255, 255], "new ink");
    assert_eq!(theme.palette().gutter, 8.0);
    Ok(())
}

#[test]
fn typography_presets_set_size_weight_and_line_height_together() -> TestResult {
    let _rt = Runtime::new().enter();
    let preset = Typography {
        font: None,
        size: 20.0,
        line_height: 2.0,
        weight: 400.0,
    };
    let (mut ui, ()) = TestUiBuilder::default().build_with(|| {
        (
            div().child(div().test_id("label").child(text("Hi").typography(preset))),
            (),
        )
    });
    assert_eq!(ui.bounds_of("label").ok_or("label")?.size.height, 40.0);
    Ok(())
}

#[test]
fn on_press_answers_clicks_and_keys_and_interactions_are_tracked() -> TestResult {
    let _rt = Runtime::new().enter();
    let presses = Rc::new(Cell::new(0));
    let outer = Rc::new(Cell::new(0));
    let state = signal(Interactions::default());
    let (mut ui, ()) = TestUiBuilder::default().build_with(|| {
        let (presses, outer) = (presses.clone(), outer.clone());
        let button = div()
            .test_id("button")
            .size(20.0)
            .autofocus()
            .track_interaction(state)
            .on_press(move || presses.set(presses.get() + 1))
            .when(true, |button| button.bg(Color::RED))
            .when_some(None::<f32>, Styled::w);
        let root = div()
            .size_full()
            .on_click(move |_| outer.set(outer.get() + 1));
        (root.child(button), ())
    });
    ui.advance(0.0);
    ui.hover_on("button")?;
    assert!(state.get_untracked().hovered);
    ui.click_on("button")?;
    ui.press(Key::Enter);
    ui.press(Key::Space);
    assert_eq!((presses.get(), outer.get()), (3, 0), "presses are consumed");
    assert!(state.get_untracked().focused());
    Ok(())
}
