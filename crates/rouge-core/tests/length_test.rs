use rouge_core::testing::{TestResult, TestUi, TestUiBuilder};
use rouge_core::{Length, Rect, Styled, auto, div, pct, px, rem, root_font_size, text};
use rouge_reactive::Runtime;

#[test]
fn lengths_resolve_as_pixels_fractions_of_the_parent_or_automatically() -> TestResult {
    let _rt = Runtime::new().enter();
    assert_eq!(
        (px(8.0), pct(25.0), auto()),
        (Length::Px(8.0), Length::Fraction(0.25), Length::Auto)
    );
    let mut ui = TestUi::new(|| {
        div()
            .size_full()
            .child(div().w(px(40.0)).h(10.0))
            .child(div().w(pct(25.0)).h(10.0))
            .child(div().w(auto()).h(10.0))
    });
    assert_eq!(ui.bounds(1.0, 5.0)?, Rect::new(0.0, 0.0, 40.0, 10.0));
    assert_eq!(ui.bounds(1.0, 15.0)?, Rect::new(0.0, 10.0, 50.0, 10.0));
    assert_eq!(ui.bounds(1.0, 25.0)?, Rect::new(0.0, 20.0, 200.0, 10.0));
    Ok(())
}

#[test]
fn rem_dimensions_follow_the_per_window_root_size() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, root_size) =
        TestUiBuilder::default().build_with(|| (div().w(rem(2.0)).h(10.0), root_font_size()));
    assert_eq!(ui.bounds(1.0, 1.0)?.size.width, 32.0);
    root_size.set(20.0);
    ui.advance(0.0);
    assert_eq!(ui.bounds(1.0, 1.0)?.size.width, 40.0);
    let mut other = TestUi::new(|| div().w(rem(2.0)).h(10.0));
    assert_eq!(other.bounds(1.0, 1.0)?.size.width, 32.0);
    Ok(())
}

#[test]
fn rem_spacing_constraints_and_text_relayout_together() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, root_size) = TestUiBuilder::default().build_with(|| {
        let view = div()
            .flex()
            .col()
            .p(rem(0.5))
            .border(rem(0.25))
            .gap(rem(0.25))
            .child(div().w(1.0).min_w(rem(2.0)).h(rem(1.0)).ml(rem(0.5)))
            .child(text("Rem").text_size(rem(1.0)));
        (view, root_font_size())
    });
    let first = ui.bounds(21.0, 13.0)?;
    assert_eq!(first, Rect::new(20.0, 12.0, 32.0, 16.0));
    assert_eq!(ui.bounds(13.0, 34.0)?.size.height, 23.0);
    root_size.set(20.0);
    ui.advance(0.0);
    assert_eq!(ui.bounds(26.0, 16.0)?, Rect::new(25.0, 15.0, 40.0, 20.0));
    assert_eq!(ui.bounds(16.0, 42.0)?.size.height, 28.0);
    Ok(())
}

#[test]
fn later_style_setters_override_rem_declarations() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, root_size) = TestUiBuilder::default()
        .build_with(|| (div().w(rem(2.0)).w_full().h(10.0), root_font_size()));
    root_size.set(20.0);
    ui.advance(0.0);
    assert_eq!(ui.bounds(1.0, 1.0)?.size.width, 200.0);
    Ok(())
}
