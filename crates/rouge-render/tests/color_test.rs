use rouge_render::Color;

#[test]
fn hex_and_hsl() {
    assert_eq!(Color::rgb(0xff8000).to_rgba8(), [255, 128, 0, 255]);
    assert_eq!(Color::from_rgba8(0, 0, 0, 128).to_rgba8(), [0, 0, 0, 128]);
    assert_eq!(Color::hsl(0.0, 1.0, 0.5).to_rgba8(), [255, 0, 0, 255]);
    assert_eq!(Color::hsl(120.0, 1.0, 0.5).to_rgba8(), [0, 255, 0, 255]);
    assert_eq!(Color::hsl(240.0, 1.0, 0.5).to_rgba8(), [0, 0, 255, 255]);
}

#[test]
fn over_composites_a_translucent_colour_onto_its_background() {
    let tint = Color::from_rgba8(0, 0, 0, 51);
    assert_eq!(tint.over(Color::WHITE).to_rgba8(), [204, 204, 204, 255]);
    assert_eq!(Color::RED.over(Color::BLUE), Color::RED);
    assert_eq!(tint.over(Color::TRANSPARENT), tint);
    assert_eq!(
        Color::TRANSPARENT.over(Color::TRANSPARENT),
        Color::TRANSPARENT
    );
}
