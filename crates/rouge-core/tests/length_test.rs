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
    for (name, setter, slots) in SETTERS {
        assert_eq!(
            setter(setter(Recorder::default(), px(7.0)), pct(50.0)).0,
            [
                Declaration::Length(slots, px(7.0)),
                Declaration::Length(slots, pct(50.0)),
            ],
            "{name}"
        );
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

type Setter = fn(Recorder, Length) -> Recorder;

const SETTERS: &[(&str, Setter, &[Slot])] = {
    use Slot::*;
    &[
        ("w", |r, v| r.w(v), &[Width]),
        ("h", |r, v| r.h(v), &[Height]),
        ("min_w", |r, v| r.min_w(v), &[MinWidth]),
        ("min_h", |r, v| r.min_h(v), &[MinHeight]),
        ("max_w", |r, v| r.max_w(v), &[MaxWidth]),
        ("max_h", |r, v| r.max_h(v), &[MaxHeight]),
        ("flex_basis", |r, v| r.flex_basis(v), &[Basis]),
        ("gap_x", |r, v| r.gap_x(v), &[GapX]),
        ("gap_y", |r, v| r.gap_y(v), &[GapY]),
        ("pt", |r, v| r.pt(v), &[PaddingTop]),
        ("pr", |r, v| r.pr(v), &[PaddingRight]),
        ("pb", |r, v| r.pb(v), &[PaddingBottom]),
        ("pl", |r, v| r.pl(v), &[PaddingLeft]),
        ("mt", |r, v| r.mt(v), &[MarginTop]),
        ("mr", |r, v| r.mr(v), &[MarginRight]),
        ("mb", |r, v| r.mb(v), &[MarginBottom]),
        ("ml", |r, v| r.ml(v), &[MarginLeft]),
        ("top", |r, v| r.top(v), &[Top]),
        ("right", |r, v| r.right(v), &[Right]),
        ("bottom", |r, v| r.bottom(v), &[Bottom]),
        ("left", |r, v| r.left(v), &[Left]),
        ("border_t", |r, v| r.border_t(v), &[BorderTop]),
        ("border_r", |r, v| r.border_r(v), &[BorderRight]),
        ("border_b", |r, v| r.border_b(v), &[BorderBottom]),
        ("border_l", |r, v| r.border_l(v), &[BorderLeft]),
        ("size", |r, v| r.size(v), &[Width, Height]),
        ("gap", |r, v| r.gap(v), &[GapX, GapY]),
        ("px", |r, v| r.px(v), &[PaddingLeft, PaddingRight]),
        ("py", |r, v| r.py(v), &[PaddingTop, PaddingBottom]),
        ("mx", |r, v| r.mx(v), &[MarginLeft, MarginRight]),
        ("my", |r, v| r.my(v), &[MarginTop, MarginBottom]),
        (
            "p",
            |r, v| r.p(v),
            &[PaddingTop, PaddingRight, PaddingBottom, PaddingLeft],
        ),
        (
            "m",
            |r, v| r.m(v),
            &[MarginTop, MarginRight, MarginBottom, MarginLeft],
        ),
        ("inset", |r, v| r.inset(v), &[Top, Right, Bottom, Left]),
        (
            "border",
            |r, v| r.border(v),
            &[BorderTop, BorderRight, BorderBottom, BorderLeft],
        ),
    ]
};
