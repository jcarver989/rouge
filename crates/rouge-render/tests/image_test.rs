use rouge_render::{ImageData, ImageError, ImageFormat};

#[test]
fn straight_alpha_is_premultiplied_and_ids_are_unique() -> Result<(), ImageError> {
    let image = ImageData::rgba8(1, 2, vec![255, 100, 0, 128, 10, 20, 30, 255])?;
    assert_eq!(image.pixels(), [128, 50, 0, 128, 10, 20, 30, 255]);
    assert_eq!(
        (image.width(), image.height(), image.format()),
        (1, 2, ImageFormat::Rgba8)
    );
    let other = ImageData::premultiplied_rgba8(1, 1, vec![1, 2, 3, 4])?;
    assert_eq!(other.pixels(), [1, 2, 3, 4]);
    assert_ne!(image, other);
    assert_eq!(image, image.clone());
    Ok(())
}

#[test]
fn mismatched_and_empty_pixel_buffers_are_rejected() {
    let mismatch = ImageData::alpha8(2, 2, vec![0; 3]);
    assert_eq!(
        mismatch,
        Err(ImageError::SizeMismatch {
            width: 2,
            height: 2,
            expected: 4,
            actual: 3
        })
    );
    assert_eq!(ImageData::rgba8(0, 4, Vec::new()), Err(ImageError::Empty));
}
