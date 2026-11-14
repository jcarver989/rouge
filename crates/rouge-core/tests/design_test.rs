use rouge_core::testing::{TestResult, TestUiBuilder, assert_near};
use rouge_core::{Apply, Color, Interactions, Styled, Typography, div, text};
use rouge_reactive::{Runtime, Signal, expect_context, provide_context, signal};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn applications_declare_their_own_theme_tokens_and_style_shorthands() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, brand) = TestUiBuilder::default().build_with(|| {
        let brand = signal(Brand {
            ink: Color::RED,
            gutter: 4.0,
        });
        provide_context(brand);
        let gutter = move || brand.get().gutter;
        (div().h(10.0).wide().inked().ml(gutter), brand)
    });
    assert_eq!(ui.bounds(5.0, 5.0)?.origin.x, 4.0);
    assert_eq!(ui.bounds(5.0, 5.0)?.size.width, 60.0);
    brand.set(Brand {
        ink: Color::WHITE,
        gutter: 8.0,
    });
    ui.advance(0.0);
    assert_near(ui.pixel(20, 5)?, [255, 255, 255, 255], "new ink");
    assert_eq!(ui.bounds(10.0, 5.0)?.origin.x, 8.0);
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
    let (ui, ()) = TestUiBuilder::default().build_with(|| {
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
    ui.press("enter");
    ui.press("space");
    assert_eq!((presses.get(), outer.get()), (3, 0), "presses are consumed");
    assert!(state.get_untracked().focused());
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
struct Brand {
    ink: Color,
    gutter: f32,
}

trait Shorthands: Styled {
    fn wide(self) -> Self {
        self.w(60.0)
    }

    fn inked(self) -> Self {
        let brand = expect_context::<Signal<Brand>>();
        self.bg(move || brand.get().ink)
    }
}

impl<T: Styled> Shorthands for T {}
