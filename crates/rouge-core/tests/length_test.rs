use rouge_core::testing::{TestResult, TestUi};
use rouge_core::{
    Declaration, Length, Prop, Rect, Slot, StyleDelta, StyleState, Styled, auto, div, pct, px,
};
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
fn every_length_setter_declares_its_slots_in_pixels_or_fractions() {
    macro_rules! check {
        ($($setter:ident => $($slot:ident),+;)*) => {$({
            let slots: &[Slot] = &[$(Slot::$slot),+];
            assert_eq!(
                Recorder::default().$setter(7.0).$setter(pct(50.0)).0,
                [
                    Declaration::Length(slots, px(7.0)),
                    Declaration::Length(slots, pct(50.0)),
                ],
                stringify!($setter)
            );
        })*};
    }
    check! {
        w => Width; h => Height;
        min_w => MinWidth; min_h => MinHeight;
        max_w => MaxWidth; max_h => MaxHeight;
        flex_basis => Basis; gap_x => GapX; gap_y => GapY;
        pt => PaddingTop; pr => PaddingRight; pb => PaddingBottom; pl => PaddingLeft;
        mt => MarginTop; mr => MarginRight; mb => MarginBottom; ml => MarginLeft;
        top => Top; right => Right; bottom => Bottom; left => Left;
        border_t => BorderTop; border_r => BorderRight; border_b => BorderBottom; border_l => BorderLeft;
        size => Width, Height; gap => GapX, GapY;
        px => PaddingLeft, PaddingRight; py => PaddingTop, PaddingBottom;
        mx => MarginLeft, MarginRight; my => MarginTop, MarginBottom;
        p => PaddingTop, PaddingRight, PaddingBottom, PaddingLeft;
        m => MarginTop, MarginRight, MarginBottom, MarginLeft;
        inset => Top, Right, Bottom, Left;
        border => BorderTop, BorderRight, BorderBottom, BorderLeft;
    }
}

#[test]
fn shorthand_setters_lay_out_every_side() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div().size_full().child(
            div()
                .flex()
                .w(100.0)
                .h(60.0)
                .p(3.0)
                .m(4.0)
                .border(1.0)
                .gap(6.0)
                .child(div().flex_1().h_full())
                .child(div().flex_1().h_full()),
        )
    });
    assert_eq!(ui.bounds(5.0, 5.0)?, Rect::new(4.0, 4.0, 100.0, 60.0));
    assert_eq!(ui.bounds(10.0, 10.0)?, Rect::new(8.0, 8.0, 43.0, 52.0));
    assert_eq!(ui.bounds(60.0, 10.0)?, Rect::new(57.0, 8.0, 43.0, 52.0));
    Ok(())
}

#[derive(Default)]
struct Recorder(Vec<Declaration>);

impl Styled for Recorder {
    fn declare(mut self, declaration: Prop<Declaration>) -> Self {
        self.0.push(declaration.get());
        self
    }

    fn restyle(self, _: StyleState, _: StyleDelta) -> Self {
        self
    }
}
