use rouge_render::{Affine, Point, Rect};
use std::f32::consts::FRAC_PI_2;

#[test]
fn affine_composes_and_inverts() -> Result<(), &'static str> {
    let t = Affine::scale(2.0, 3.0).then(&Affine::translate(10.0, 20.0));
    assert!(near(t.apply(Point::new(1.0, 1.0)), Point::new(12.0, 23.0)));
    let inverse = t
        .inverse()
        .ok_or("a non-degenerate transform has an inverse")?;
    assert!(near(
        inverse.apply(Point::new(12.0, 23.0)),
        Point::new(1.0, 1.0)
    ));
    assert!(Affine::scale(0.0, 1.0).inverse().is_none());

    let quarter_turn = Affine::rotate(FRAC_PI_2);
    assert!(near(
        quarter_turn.apply(Point::new(1.0, 0.0)),
        Point::new(0.0, 1.0)
    ));

    let about = Affine::scale(2.0, 2.0).about(Point::new(50.0, 50.0));
    assert!(near(
        about.apply(Point::new(50.0, 50.0)),
        Point::new(50.0, 50.0)
    ));
    assert!(near(
        about.apply(Point::new(60.0, 50.0)),
        Point::new(70.0, 50.0)
    ));
    Ok(())
}

#[test]
fn intersect_and_contains() {
    let a = Rect::new(0.0, 0.0, 10.0, 10.0);
    let b = Rect::new(5.0, 5.0, 10.0, 10.0);
    assert_eq!(a.intersect(&b), Rect::new(5.0, 5.0, 5.0, 5.0));
    assert!(a.intersect(&Rect::new(20.0, 0.0, 5.0, 5.0)).is_empty());
    assert!(a.contains(Point::new(0.0, 0.0)));
    assert!(!a.contains(Point::new(10.0, 5.0)));
    assert_eq!(a.intersect(&Rect::EVERYTHING), a);
}

fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
}
