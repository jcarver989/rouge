use rouge_render::Color;

#[test]
fn hex_and_hsl() {
    assert_eq!(Color::rgb(0xff8000).to_rgba8(), [255, 128, 0, 255]);
    assert_eq!(Color::from_rgba8(0, 0, 0, 128).to_rgba8(), [0, 0, 0, 128]);
    assert_eq!(Color::hsl(0.0, 1.0, 0.5).to_rgba8(), [255, 0, 0, 255]);
    assert_eq!(Color::hsl(120.0, 1.0, 0.5).to_rgba8(), [0, 255, 0, 255]);
    assert_eq!(Color::hsl(240.0, 1.0, 0.5).to_rgba8(), [0, 0, 255, 255]);
}
