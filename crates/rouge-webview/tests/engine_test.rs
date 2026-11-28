use rouge_core::{Corners, PaintRegion, Rect, Size};
use rouge_webview::{PageScale, Placement, WebContent};

#[test]
fn a_layout_is_the_clip_with_the_page_inside_it_and_only_the_corners_in_view_round()
-> Result<(), &'static str> {
    let frame = Rect::new(10.0, 20.0, 200.0, 100.0);
    for (clip, page, radii) in [
        (frame, Rect::new(0.0, 0.0, 200.0, 100.0), [12.0; 4]),
        (
            Rect::new(10.0, 60.0, 200.0, 60.0),
            Rect::new(0.0, -40.0, 200.0, 100.0),
            [0.0, 0.0, 12.0, 12.0],
        ),
        (
            Rect::new(10.0, 20.0, 150.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 100.0),
            [12.0, 0.0, 0.0, 12.0],
        ),
    ] {
        let placement = Placement {
            frame,
            clip,
            corners: Corners::all(12.0),
            page_zoom: 0.5,
            ..Placement::default()
        };
        let layout = placement.layout(1.0).ok_or("a visible page has a layout")?;
        assert_eq!((layout.clip, layout.page), (clip, page));
        assert_eq!(layout.corners.to_array(), radii);
        assert_eq!(layout.unzoomed, Size::new(400.0, 200.0));
        let doubled = placement.layout(2.0).ok_or("a visible page has a layout")?;
        assert_eq!(doubled.page.size, Size::new(400.0, 200.0));
        assert_eq!(doubled.unzoomed, Size::new(800.0, 400.0));
        assert_eq!(doubled.corners.to_array(), radii.map(|radius| radius * 2.0));
        assert_eq!(doubled.zoom, 0.5, "the page zoom is not a length");
    }
    assert_eq!(Placement::default().layout(1.0), None, "a hidden page");
    Ok(())
}

#[test]
fn a_transparent_page_keeps_its_layout_but_is_not_visible() -> Result<(), &'static str> {
    let placement = Placement {
        frame: Rect::new(0.0, 0.0, 300.0, 200.0),
        clip: Rect::new(0.0, 0.0, 300.0, 200.0),
        opacity: 0.0,
        ..Placement::default()
    };
    assert!(!placement.visible());
    let layout = placement
        .layout(1.0)
        .ok_or("a transparent page is still laid out")?;
    assert_eq!(layout.unzoomed, Size::new(300.0, 200.0));
    assert!(
        Placement {
            opacity: 0.4,
            ..placement
        }
        .visible()
    );
    Ok(())
}

#[test]
fn a_placement_is_its_paint_region_in_window_points() {
    let region = PaintRegion {
        frame: Rect::new(10.0, 20.0, 200.0, 100.0),
        bounds: Rect::new(10.0, 20.0, 200.0, 60.0),
        corners: Corners::all(8.0),
        ..PaintRegion::default()
    };
    assert_eq!(
        Placement::from_region(&region, 2.0, 3.0, PageScale::Zoom(0.5), true),
        Placement {
            frame: Rect::new(20.0, 40.0, 400.0, 200.0),
            clip: Rect::new(20.0, 40.0, 400.0, 120.0),
            corners: Corners::all(16.0),
            device_scale: 3.0,
            page_zoom: 1.0,
            opacity: 1.0,
            interactive: true,
        }
    );
}

#[test]
fn a_page_fitted_to_a_size_keeps_that_layout_size_whatever_its_frame() -> Result<(), &'static str> {
    let fitted = Size::new(900.0, 600.0);
    for (width, height) in [
        (300.0, 169.0),
        (451.0, 277.0),
        (743.0, 512.0),
        (900.0, 600.0),
    ] {
        let region = PaintRegion {
            frame: Rect::new(40.0, 50.0, width, 900.0),
            bounds: Rect::new(40.0, 50.0, width, height),
            ..PaintRegion::default()
        };
        let placement = Placement::from_region(&region, 1.0, 2.0, PageScale::Fit(fitted), true);
        assert_eq!(placement.page_zoom, width / 900.0);
        assert_eq!(placement.clip.size, Size::new(width, height));
        let layout = placement.layout(1.0).ok_or("a visible page has a layout")?;
        assert_eq!(
            layout.unzoomed, fitted,
            "the web view would resize at {width} wide"
        );
    }
    Ok(())
}

#[test]
fn only_html_content_takes_a_base_url() {
    assert_eq!(
        WebContent::html("<p>hi</p>").base_url("https://example.com/"),
        WebContent::Html {
            html: "<p>hi</p>".into(),
            base_url: Some("https://example.com/".into()),
        }
    );
    assert_eq!(
        WebContent::url("https://example.com/a").base_url("https://other.example/"),
        WebContent::Url("https://example.com/a".into())
    );
}
