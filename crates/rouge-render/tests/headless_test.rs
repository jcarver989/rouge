mod common;

use common::{CLEAR, Canvas, RED, TestResult, quads};
use image::load_from_memory;
use rouge_render::headless::assert_near;
use rouge_render::{Color, LayerDraw, Quad, Rect};

#[test]
fn frames_read_back_at_device_size_and_round_trip_through_png() -> TestResult {
    let list = quads([Quad::new(Rect::new(0.0, 0.0, 3.0, 2.0), Color::RED)]);
    let mut canvas = Canvas::new(5, 2.0, Color::TRANSPARENT)?;
    canvas.frame(&[LayerDraw::new(&list)]);
    let img = canvas.image()?;
    assert_eq!((img.width, img.height, img.pixels.len()), (5, 5, 100));
    assert_near(img.pixel(4, 3), RED, "inside the quad at 2x");
    assert_eq!(img.pixel(4, 4), CLEAR, "below the quad");

    let decoded = load_from_memory(&img.to_png()?)?.into_rgba8();
    assert_eq!(decoded.dimensions(), (5, 5));
    assert_eq!(decoded.into_raw(), img.pixels);
    Ok(())
}
