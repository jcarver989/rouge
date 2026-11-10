use rouge_core::{Path, Rect, Size};

#[test]
fn paths_compare_by_content_and_fit_a_view_box() {
    let square = Path::builder()
        .rect(Rect::new(0.0, 0.0, 10.0, 10.0))
        .build();
    assert_eq!(square, Path::svg("M0 0 L10 0 L10 10 L0 10 Z"));
    assert_ne!(
        square,
        Path::builder()
            .rect(Rect::new(0.0, 0.0, 10.0, 11.0))
            .build()
    );
    assert!(Path::builder().build().is_empty());
    let fitted = square.fit(Size::new(10.0, 10.0), Rect::new(5.0, 0.0, 40.0, 20.0));
    assert_eq!(
        fitted,
        Path::builder()
            .rect(Rect::new(15.0, 0.0, 20.0, 20.0))
            .build(),
        "scaled and centered"
    );
}
