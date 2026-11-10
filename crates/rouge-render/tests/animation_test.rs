use rouge_render::{Color, Ease, Keyframes, LayerProp, Pose};

#[test]
fn easing_curves_hit_their_endpoints() {
    for ease in [Ease::Linear, Ease::In, Ease::Out, Ease::InOut] {
        assert_eq!(ease.apply(0.0), 0.0);
        assert!((ease.apply(1.0) - 1.0).abs() < 1e-6);
        assert_eq!(ease.apply(2.0), ease.apply(1.0), "clamped");
    }
    assert!(Ease::In.apply(0.5) < 0.5 && Ease::Out.apply(0.5) > 0.5);
    assert!((Ease::InOut.apply(0.5) - 0.5).abs() < 1e-6);
}

#[test]
fn cubic_bezier_follows_its_control_points() {
    let linear = Ease::cubic_bezier(0.0, 0.0, 1.0, 1.0);
    let ease = Ease::cubic_bezier(0.25, 0.1, 0.25, 1.0);
    let back = Ease::cubic_bezier(0.3, 1.6, 0.6, 1.0);
    for step in 0..=10 {
        let t = step as f32 / 10.0;
        assert!(
            (linear.apply(t) - t).abs() < 1e-3,
            "the diagonal is linear at {t}"
        );
    }
    assert_eq!((ease.apply(0.0), ease.apply(1.0)), (0.0, 1.0));
    assert!(
        (ease.apply(0.5) - 0.8024).abs() < 2e-3,
        "CSS `ease` at the midpoint: {}",
        ease.apply(0.5)
    );
    assert!(
        (0..=20)
            .map(|i| back.apply(i as f32 / 20.0))
            .any(|v| v > 1.0),
        "control points may overshoot"
    );
}

#[test]
fn keyframes_interpolate_between_stops() {
    let keyframes = Keyframes::new()
        .at(1.0, |pose| pose.translate_x(100.0).opacity(0.0))
        .at(0.0, |pose| pose.translate_x(0.0))
        .at(0.5, |pose| pose.translate_x(80.0));
    assert_eq!(keyframes.sample(LayerProp::TranslateX, 0.25), Some(40.0));
    assert_eq!(keyframes.sample(LayerProp::TranslateX, 0.75), Some(90.0));
    assert_eq!(keyframes.sample(LayerProp::TranslateX, 1.0), Some(100.0));
    assert_eq!(
        keyframes.sample(LayerProp::Opacity, 0.2),
        Some(0.0),
        "a single stop holds its value"
    );
    assert_eq!(
        keyframes.sample(LayerProp::Scale, 0.5),
        None,
        "untouched props are left alone"
    );
}

#[test]
fn poses_list_only_the_props_they_set() {
    let pose = Pose::new().opacity(0.0).scale(0.9);
    let set: Vec<_> = pose.iter().collect();
    assert_eq!(set, [(LayerProp::Scale, 0.9), (LayerProp::Opacity, 0.0)]);
    assert_eq!(pose.get(LayerProp::Rotate), None);

    let styled = Pose::new()
        .background(Color::RED.with_alpha(0.5))
        .rounded(6.0);
    let fill = LayerProp::FILL.map(|prop| styled.get(prop));
    assert_eq!(
        fill,
        [Some(0.5), Some(0.0), Some(0.0), Some(0.5)],
        "colors are premultiplied"
    );
    assert!(
        LayerProp::RADII
            .iter()
            .all(|prop| styled.get(*prop) == Some(6.0))
    );
}
