use rouge_render::{Atlas, AtlasRect};

#[test]
fn rects_do_not_overlap() -> Result<(), &'static str> {
    let mut atlas = Atlas::new();
    let mut rects: Vec<AtlasRect> = Vec::new();
    for i in 0..500u32 {
        let (w, h) = (3 + i % 17, 5 + i % 11);
        let rect = atlas
            .insert(w, h, &vec![1; (w * h) as usize])
            .ok_or("atlas has room")?;
        assert_eq!((rect.width, rect.height), (w, h));
        assert!(
            rects.iter().all(|other| !overlaps(&rect, other)),
            "{rect:?} overlaps another"
        );
        rects.push(rect);
    }
    Ok(())
}

#[test]
fn grows_downward_without_moving_existing_rects() -> Result<(), &'static str> {
    let mut atlas = Atlas::new();
    let first = atlas
        .insert(100, 100, &[7; 10_000])
        .ok_or("atlas has room")?;
    let (width, initial_height) = atlas.size();
    let mut last = first;
    while atlas.size().1 == initial_height {
        last = atlas
            .insert(1000, 100, &vec![1; 100_000])
            .ok_or("atlas can grow")?;
    }
    assert_eq!(atlas.size().0, width);
    assert!(
        last.y + last.height > initial_height,
        "new space is below the old"
    );
    assert!(!overlaps(&first, &last));
    Ok(())
}

#[test]
fn rejects_oversized_and_empty_masks() {
    let mut atlas = Atlas::new();
    assert!(
        atlas
            .insert(Atlas::WIDTH, 1, &vec![0; Atlas::WIDTH as usize])
            .is_none()
    );
    assert!(atlas.insert(0, 4, &[]).is_none());
}

fn overlaps(a: &AtlasRect, b: &AtlasRect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}
