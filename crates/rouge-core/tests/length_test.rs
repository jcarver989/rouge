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

#[test]
fn every_length_setter_tracks_rem_and_clears_it_for_absolute_overrides() {
    macro_rules! check {
        ($($setter:ident => $field:ident $(.$side:ident)?),* $(,)?) => {$({
            let mut view = div().$setter(rem(2.0));
            assert_eq!(view.style_mut().rem_lengths.get(stringify!($field $(.$side)?)), Some(&2.0));
            let mut view = view.$setter(7.0);
            let style = view.style_mut();
            assert!(style.rem_lengths.is_empty(), stringify!($setter));
            assert_eq!(style.layout.$field $(.$side)?, Length::Px(7.0).into());
        })*};
    }
    check! {
        w => size.width, h => size.height,
        min_w => min_size.width, min_h => min_size.height,
        max_w => max_size.width, max_h => max_size.height,
        flex_basis => flex_basis, gap_x => gap.width, gap_y => gap.height,
        pt => padding.top, pr => padding.right, pb => padding.bottom, pl => padding.left,
        mt => margin.top, mr => margin.right, mb => margin.bottom, ml => margin.left,
        top => inset.top, right => inset.right, bottom => inset.bottom, left => inset.left,
        border_t => border.top, border_r => border.right, border_b => border.bottom, border_l => border.left,
    }
    let mut view = div()
        .size(rem(2.0))
        .size_full()
        .p(rem(2.0))
        .px(3.0)
        .py(4.0)
        .m(rem(2.0))
        .mx(3.0)
        .my(4.0)
        .inset(rem(2.0))
        .inset(5.0)
        .border(rem(2.0))
        .border(1.0)
        .gap(rem(2.0))
        .gap(6.0)
        .flex_basis(rem(2.0))
        .flex_1();
    assert!(view.style_mut().rem_lengths.is_empty());
}
