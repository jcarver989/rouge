use rouge_core::testing::{BLUE, CLEAR, RED, TestResult, TestUiBuilder, assert_near, recorder};
use rouge_core::{
    Fit, ImageData, ImageError, ImageLoadError, ImageSource, Rect, Styled, decode_image, div, image,
};
use rouge_reactive::{Runtime, signal};
use rouge_render::headless::Image as Pixels;
use std::error::Error;

#[test]
fn an_image_takes_its_natural_size_and_keeps_its_ratio_when_one_side_is_set() -> TestResult {
    let _rt = Runtime::new().enter();
    let wide = wide()?;
    let mut ui = TestUiBuilder::default().build(|| {
        div()
            .flex()
            .items_start()
            .child(image(wide.clone()).flex_none())
            .child(image(wide).w(80.0).flex_none())
    });
    ui.advance(0.0);
    assert_eq!(ui.bounds(1.0, 1.0)?, Rect::new(0.0, 0.0, 4.0, 2.0));
    assert_eq!(ui.bounds(50.0, 20.0)?, Rect::new(4.0, 0.0, 80.0, 40.0));
    let img = ui.render()?;
    assert_near(img.pixel(10, 20), RED, "left half");
    assert_near(img.pixel(80, 20), BLUE, "right half");
    Ok(())
}

#[test]
fn fit_modes_contain_cover_and_fill() -> TestResult {
    let _rt = Runtime::new().enter();
    let wide = wide()?;
    let mut ui = TestUiBuilder::default().size(300.0, 100.0).build(|| {
        let cell = |fit: Fit| image(wide.clone()).fit(fit).size(100.0).flex_none();
        div()
            .flex()
            .child(cell(Fit::Contain))
            .child(cell(Fit::Cover).rounded(50.0))
            .child(cell(Fit::Fill))
    });
    ui.advance(0.0);
    let img = ui.render()?;
    assert_near(img.pixel(10, 10), CLEAR, "contain letterboxes");
    assert_near(img.pixel(10, 50), RED, "contain fills the width");
    assert_near(img.pixel(90, 50), BLUE, "contain keeps both halves");
    assert_near(
        img.pixel(103, 3),
        CLEAR,
        "cover is clipped by the rounded corners",
    );
    assert_near(
        img.pixel(150, 5),
        [125, 0, 130, 255],
        "cover fills the height, so the seam is centered",
    );
    assert_near(img.pixel(115, 50), RED, "cover crops the sides");
    assert_near(img.pixel(185, 50), BLUE, "cover crops the sides");
    assert_near(img.pixel(210, 5), RED, "fill stretches to the box");
    assert_near(img.pixel(290, 95), BLUE, "fill stretches to the box");
    Ok(())
}

#[test]
fn encoded_images_decode_off_the_ui_thread_and_errors_are_reported() -> TestResult {
    let _rt = Runtime::new().enter();
    let png = encoded()?;
    assert_eq!(decode_image(&png)?.width(), 4);
    let (errors, record) = recorder();
    let source = signal(ImageSource::Bytes(png.into()));
    let mut ui = TestUiBuilder::default().build(move || {
        div().child(image(source).w(80.0).h(40.0)).child(
            image(vec![1u8, 2, 3])
                .on_error(move |error| record(matches!(error, ImageLoadError::Decode(_)))),
        )
    });
    ui.wait_until(|ui| ui.pixel(10, 20).is_ok_and(|p| p == RED) && !errors.borrow().is_empty());
    assert_near(ui.pixel(10, 20)?, RED, "decoded image is shown");
    assert_eq!(*errors.borrow(), [true], "the bad bytes fail to decode");
    source.set(ImageSource::File("/nonexistent/rouge.png".into()));
    ui.wait_until(|ui| ui.pixel(10, 20).is_ok_and(|p| p == CLEAR));
    assert_near(ui.pixel(10, 20)?, CLEAR, "a failed load clears the image");
    Ok(())
}

fn wide() -> Result<ImageData, ImageError> {
    let (red, blue) = ([255u8, 0, 0, 255], [0u8, 0, 255, 255]);
    let row = [red, red, blue, blue].concat();
    ImageData::rgba8(4, 2, [row.clone(), row].concat())
}

fn encoded() -> Result<Vec<u8>, Box<dyn Error>> {
    let source = wide()?;
    let pixels = Pixels {
        width: source.width(),
        height: source.height(),
        pixels: source.pixels().to_vec(),
    };
    Ok(pixels.to_png()?)
}
