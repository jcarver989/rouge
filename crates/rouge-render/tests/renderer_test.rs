mod common;

use common::{BLUE, CLEAR, Canvas, RED, WHITE, backdrop, full, quads, render_layers, split_scene};
use rouge_render::headless::{Image, assert_near};
use rouge_render::{
    Affine, Color, Corners, DrawList, Edges, EffectDraw, Glass, Glyph, GpuError, ImageData,
    ImageQuad, LayerDraw, Point, Quad, Rect, Shadow,
};
use std::error::Error;
use std::f32::consts::FRAC_PI_4;

#[test]
fn solid_quad_fills_exactly_its_bounds() -> Result<(), GpuError> {
    let list = quads([Quad::new(Rect::new(10.0, 20.0, 30.0, 40.0), Color::RED)]);
    let img = render(1.0, 100, Color::TRANSPARENT, &list)?;
    assert_near(img.pixel(10, 20), RED, "top-left inside");
    assert_near(img.pixel(39, 59), RED, "bottom-right inside");
    assert_near(img.pixel(25, 40), RED, "center");
    assert_near(img.pixel(9, 20), CLEAR, "left of quad");
    assert_near(img.pixel(10, 19), CLEAR, "above quad");
    assert_near(img.pixel(40, 59), CLEAR, "right of quad");
    assert_near(img.pixel(39, 60), CLEAR, "below quad");
    Ok(())
}

#[test]
fn scale_factor_maps_logical_to_device_pixels() -> Result<(), GpuError> {
    let list = quads([Quad::new(Rect::new(10.0, 10.0, 20.0, 20.0), Color::RED)]);
    let img = render(2.0, 100, Color::WHITE, &list)?;
    assert_near(img.pixel(20, 20), RED, "device origin is doubled");
    assert_near(img.pixel(59, 59), RED, "device extent is doubled");
    assert_near(img.pixel(19, 19), WHITE, "outside");
    assert_near(img.pixel(60, 60), WHITE, "outside");
    Ok(())
}

#[test]
fn rounded_corners_are_cut_and_antialiased() -> Result<(), GpuError> {
    let list = quads([Quad {
        corner_radii: Corners::all(50.0),
        ..full(Color::RED)
    }]);
    let img = render(1.0, 100, Color::TRANSPARENT, &list)?;
    assert_near(img.pixel(50, 50), RED, "center of circle");
    assert_near(img.pixel(2, 2), CLEAR, "outside the rounded corner");
    assert_near(img.pixel(97, 97), CLEAR, "outside the opposite corner");
    assert_near(img.pixel(50, 0), RED, "top of circle");
    let partial = (0..50).any(|i| (17..240).contains(&img.pixel(i, i)[3]));
    assert!(partial, "expected an antialiased edge pixel");
    Ok(())
}

#[test]
fn per_corner_radii_are_independent() -> Result<(), GpuError> {
    let corner_radii = Corners {
        top_left: 40.0,
        ..Corners::ZERO
    };
    let img = render(
        1.0,
        100,
        Color::TRANSPARENT,
        &quads([Quad {
            corner_radii,
            ..full(Color::RED)
        }]),
    )?;
    assert_near(img.pixel(3, 3), CLEAR, "rounded top-left");
    assert_near(img.pixel(96, 3), RED, "square top-right");
    assert_near(img.pixel(96, 96), RED, "square bottom-right");
    assert_near(img.pixel(3, 96), RED, "square bottom-left");
    Ok(())
}

#[test]
fn border_is_drawn_inside_bounds_with_per_side_widths() -> Result<(), GpuError> {
    let list = quads([Quad {
        border_color: Color::BLUE,
        border_widths: Edges {
            top: 10.0,
            right: 0.0,
            bottom: 4.0,
            left: 20.0,
        },
        ..full(Color::RED)
    }]);
    let img = render(1.0, 100, Color::TRANSPARENT, &list)?;
    assert_near(img.pixel(50, 5), BLUE, "top border");
    assert_near(img.pixel(50, 12), RED, "just below top border");
    assert_near(img.pixel(10, 50), BLUE, "left border");
    assert_near(img.pixel(22, 50), RED, "just right of left border");
    assert_near(img.pixel(97, 50), RED, "no right border");
    assert_near(img.pixel(50, 97), BLUE, "bottom border");
    assert_near(img.pixel(50, 94), RED, "just above bottom border");
    Ok(())
}

#[test]
fn translucent_quads_blend_in_gamma_space() -> Result<(), GpuError> {
    let list = quads([Quad::new(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Color::BLACK.with_alpha(0.5),
    )]);
    let img = render(1.0, 10, Color::WHITE, &list)?;
    assert_near(
        img.pixel(5, 5),
        [128, 128, 128, 255],
        "mid-gray as in CSS, not linear-light 188",
    );
    Ok(())
}

#[test]
fn clip_rect_limits_drawing() -> Result<(), GpuError> {
    let list = quads([Quad {
        clip: Rect::new(20.0, 30.0, 10.0, 10.0),
        ..full(Color::RED)
    }]);
    let img = render(1.0, 100, Color::TRANSPARENT, &list)?;
    assert_near(img.pixel(20, 30), RED, "inside clip");
    assert_near(img.pixel(29, 39), RED, "inside clip");
    assert_near(img.pixel(19, 30), CLEAR, "outside clip");
    assert_near(img.pixel(30, 39), CLEAR, "outside clip");
    Ok(())
}

#[test]
fn later_primitives_draw_on_top_across_kinds() -> Result<(), GpuError> {
    let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
    let mut list = quads([Quad::new(bounds, Color::RED)]);
    list.push_shadow(Shadow::new(bounds, Color::BLACK, 0.0));
    list.push_quad(Quad::new(Rect::new(0.0, 0.0, 5.0, 10.0), Color::BLUE));
    let img = render(1.0, 10, Color::TRANSPARENT, &list)?;
    assert_near(img.pixel(2, 5), BLUE, "last quad wins");
    assert_near(
        img.pixel(7, 5),
        [0, 0, 0, 255],
        "the shadow covers the first quad",
    );
    Ok(())
}

#[test]
fn shadow_is_soft_and_falls_off_with_distance() -> Result<(), GpuError> {
    let mut list = DrawList::new();
    list.push_shadow(Shadow {
        corner_radii: Corners::all(4.0),
        ..Shadow::new(Rect::new(40.0, 40.0, 40.0, 40.0), Color::BLACK, 16.0)
    });
    let img = render(1.0, 120, Color::TRANSPARENT, &list)?;
    let alpha = |x, y| img.pixel(x, y)[3];
    assert!(
        alpha(60, 60) > 200,
        "dense in the middle, got {}",
        alpha(60, 60)
    );
    let edge = alpha(40, 60);
    assert!(
        (100..156).contains(&edge),
        "a Gaussian covers about half at the edge, got {edge}"
    );
    assert!(
        alpha(34, 60) < edge && alpha(34, 60) > alpha(26, 60),
        "monotonic falloff"
    );
    assert_eq!(alpha(5, 60), 0, "nothing far away");
    for (x, y) in [(79, 60), (60, 40), (60, 79)] {
        assert!(
            (i32::from(alpha(x, y)) - i32::from(edge)).abs() <= 12,
            "symmetry at {x},{y}"
        );
    }
    Ok(())
}

#[test]
fn glyph_masks_are_tinted_and_pixel_exact() -> Result<(), Box<dyn Error>> {
    let draw = |scale| {
        let mut canvas = Canvas::new(16, scale, Color::WHITE)?;
        let atlas = canvas.atlas();
        atlas.insert(40, 9, &[255; 360]).ok_or("atlas has room")?;
        let mask = atlas
            .insert(3, 2, &[255, 128, 0, 0, 128, 255])
            .ok_or("atlas has room")?;
        let mut list = DrawList::new();
        list.push_glyph(Glyph {
            origin: Point::new(4.0, 6.0),
            clip: Rect::EVERYTHING,
            color: Color::RED,
            mask,
        });
        canvas.frame(&[LayerDraw::new(&list)]);
        Ok::<_, Box<dyn Error>>(canvas.image()?)
    };
    let img = draw(1.0)?;
    assert_near(img.pixel(4, 6), RED, "full coverage");
    assert_near(
        img.pixel(5, 6),
        [255, 127, 127, 255],
        "half coverage over white",
    );
    assert_near(img.pixel(6, 6), WHITE, "no coverage");
    assert_near(img.pixel(4, 7), WHITE, "second row starts empty");
    assert_near(img.pixel(6, 7), RED, "second row ends full");
    assert_near(img.pixel(3, 6), WHITE, "left of the mask");
    assert_near(img.pixel(7, 7), WHITE, "right of the mask");
    assert_near(img.pixel(4, 8), WHITE, "below the mask");

    let img = draw(2.0)?;
    assert_near(img.pixel(8, 12), RED, "origin scaled");
    assert_near(img.pixel(10, 13), RED, "one texel per device pixel");
    assert_near(img.pixel(11, 13), WHITE, "not stretched");
    Ok(())
}

#[test]
fn glyphs_respect_clip_and_atlas_updates_reach_the_gpu() -> Result<(), Box<dyn Error>> {
    let mut canvas = Canvas::new(8, 1.0, Color::WHITE)?;
    let first = canvas
        .atlas()
        .insert(2, 2, &[255; 4])
        .ok_or("atlas has room")?;
    let mut list = DrawList::new();
    list.push_glyph(Glyph {
        origin: Point::ZERO,
        clip: Rect::EVERYTHING,
        color: Color::RED,
        mask: first,
    });
    canvas.frame(&[LayerDraw::new(&list)]);

    let second = canvas
        .atlas()
        .insert(4, 4, &[255; 16])
        .ok_or("atlas has room")?;
    list.clear();
    list.push_glyph(Glyph {
        origin: Point::ZERO,
        clip: Rect::new(0.0, 0.0, 2.0, 4.0),
        color: Color::BLUE,
        mask: second,
    });
    canvas.frame(&[LayerDraw::new(&list)]);
    let img = canvas.image()?;
    assert_near(
        img.pixel(1, 3),
        BLUE,
        "the mask added after the first frame was uploaded",
    );
    assert_near(img.pixel(2, 3), WHITE, "clipped");
    Ok(())
}

#[test]
fn layer_transform_moves_scales_and_rotates_content() -> Result<(), GpuError> {
    let draw = |transform| {
        let list = square(Color::RED);
        render_layers(
            100,
            &[LayerDraw {
                transform,
                ..LayerDraw::new(&list)
            }],
        )
    };

    let moved = draw(Affine::translate(40.0, 30.0))?;
    assert_near(moved.pixel(50, 40), RED, "translated origin");
    assert_near(moved.pixel(69, 59), RED, "translated extent");
    assert_near(moved.pixel(15, 15), CLEAR, "old position is empty");
    assert_near(moved.pixel(70, 59), CLEAR, "exact edge is preserved");

    let center = Point::new(20.0, 20.0);
    let scaled = draw(Affine::scale(2.0, 2.0).about(center))?;
    assert_near(scaled.pixel(0, 0), RED, "grew to the corner");
    assert_near(scaled.pixel(39, 39), RED, "doubled extent");
    assert_near(scaled.pixel(40, 40), CLEAR, "but no further");

    let rotated = draw(Affine::rotate(FRAC_PI_4).about(center))?;
    assert_near(rotated.pixel(20, 20), RED, "center stays covered");
    assert_near(
        rotated.pixel(11, 11),
        CLEAR,
        "corners are cut off by a 45 degree turn",
    );
    assert_near(
        rotated.pixel(20, 8),
        RED,
        "the diamond's top point extends past the old edge",
    );
    let soft = (0..40).any(|x| (21..235).contains(&rotated.pixel(x, 14)[3]));
    assert!(soft, "rotated edges are antialiased");
    Ok(())
}

#[test]
fn layer_opacity_fades_content_and_layers_stack_in_order() -> Result<(), GpuError> {
    let red = square(Color::RED);
    let blue = square(Color::BLUE);
    let faded = LayerDraw {
        transform: Affine::translate(10.0, 0.0),
        opacity: 0.5,
        ..LayerDraw::new(&blue)
    };
    let img = render_layers(40, &[LayerDraw::new(&red), faded])?;
    assert_near(img.pixel(12, 20), RED, "only the bottom layer");
    assert_near(
        img.pixel(25, 20),
        [127, 0, 127, 255],
        "half-transparent blue over red",
    );
    assert_near(
        img.pixel(35, 20),
        [0, 0, 127, 127],
        "half-transparent blue over nothing",
    );

    let hidden = render_layers(
        40,
        &[LayerDraw {
            opacity: 0.0,
            ..LayerDraw::new(&square(Color::RED))
        }],
    )?;
    assert_near(
        hidden.pixel(20, 20),
        CLEAR,
        "fully transparent layer draws nothing",
    );
    Ok(())
}

#[test]
fn unchanged_lists_are_not_uploaded_again() -> Result<(), GpuError> {
    let mut canvas = Canvas::new(64, 1.0, Color::WHITE)?;
    let mut a = square(Color::RED);
    let b = square(Color::BLUE);
    let mut frame = |a: &DrawList, x: f32| {
        canvas.frame(&[
            LayerDraw::new(a),
            LayerDraw {
                transform: Affine::translate(x, 0.0),
                ..LayerDraw::new(&b)
            },
        ])
    };

    let first = frame(&a, 0.0);
    assert_eq!(
        first.instance_bytes_uploaded,
        2 * INSTANCE_BYTES,
        "both lists uploaded once"
    );
    assert_eq!((first.layers, first.draw_calls), (2, 2));
    for x in 1..10 {
        assert_eq!(
            frame(&a, x as f32).instance_bytes_uploaded,
            0,
            "transforms send no instances"
        );
    }

    a.clear();
    a.push_quad(Quad::new(Rect::new(0.0, 0.0, 5.0, 5.0), Color::RED));
    assert_eq!(
        frame(&a, 0.0).instance_bytes_uploaded,
        INSTANCE_BYTES,
        "only the rebuilt list"
    );
    assert_eq!(frame(&a, 0.0).instance_bytes_uploaded, 0);

    a.clear();
    for i in 0..1000 {
        let at = Rect::new((i % 50) as f32, (40 + i / 50) as f32, 1.0, 1.0);
        a.push_quad(Quad::new(at, Color::RED));
    }
    assert_eq!(
        frame(&a, 0.0).instance_bytes_uploaded,
        1000 * INSTANCE_BYTES,
        "a longer list grows the buffer"
    );
    let img = canvas.image()?;
    assert_near(img.pixel(0, 40), RED, "first");
    assert_near(img.pixel(49, 59), RED, "last");
    assert_near(img.pixel(0, 60), WHITE, "beyond");
    Ok(())
}

#[test]
fn many_layers_grow_the_uniform_buffer() -> Result<(), GpuError> {
    let list = quads([Quad::new(Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE)]);
    let layers: Vec<_> = (0..200)
        .map(|i| LayerDraw {
            transform: Affine::translate((i % 20) as f32, (i / 20) as f32),
            ..LayerDraw::new(&list)
        })
        .collect();
    let img = render_layers(20, &layers)?;
    assert_near(img.pixel(0, 0), WHITE, "first layer");
    assert_near(img.pixel(19, 9), WHITE, "last layer");
    assert_near(img.pixel(0, 10), CLEAR, "beyond the last layer");
    Ok(())
}

#[test]
fn backdrop_passes_are_counted_and_absent_without_effects() -> Result<(), GpuError> {
    let mut canvas = Canvas::new(64, 1.0, Color::WHITE)?;
    let (scene, empty) = (split_scene(), DrawList::new());
    let glass = LayerDraw {
        backdrop: Some(backdrop(Rect::new(0.0, 0.0, 20.0, 20.0), 8.0)),
        ..LayerDraw::new(&empty)
    };

    let plain = canvas.frame(&[LayerDraw::new(&scene)]);
    assert_eq!(plain.backdrop_passes, 0, "no effects, no offscreen work");
    let two = canvas.frame(&[LayerDraw::new(&scene), glass, glass]);
    assert_eq!(two.backdrop_passes, 2);
    let hidden = canvas.frame(&[
        LayerDraw::new(&scene),
        LayerDraw {
            opacity: 0.0,
            ..glass
        },
    ]);
    assert_eq!(
        hidden.backdrop_passes, 0,
        "a hidden glass layer costs nothing"
    );
    Ok(())
}

#[test]
fn backdrops_that_do_not_overlap_share_one_pass_break() -> Result<(), GpuError> {
    let mut canvas = Canvas::new(64, 1.0, Color::WHITE)?;
    let (scene, empty) = (split_scene(), DrawList::new());
    let glass = |x: f32, y: f32| LayerDraw {
        backdrop: Some(backdrop(Rect::new(x, y, 16.0, 16.0), 2.0)),
        ..LayerDraw::new(&empty)
    };

    let stats = canvas.frame(&[LayerDraw::new(&scene), glass(0.0, 0.0), glass(40.0, 40.0)]);
    assert_eq!(
        (stats.backdrop_passes, stats.backdrop_blurs),
        (1, 2),
        "one break serves both"
    );

    let stats = canvas.frame(&[LayerDraw::new(&scene), glass(4.0, 4.0), glass(14.0, 14.0)]);
    assert_eq!(
        (stats.backdrop_passes, stats.backdrop_blurs),
        (2, 2),
        "the second must see the first"
    );
    Ok(())
}

#[test]
fn a_backdrop_is_reused_until_something_behind_it_changes() -> Result<(), GpuError> {
    let mut canvas = Canvas::new(64, 1.0, Color::WHITE)?;
    let (scene, empty, above) = (split_scene(), DrawList::new(), square(Color::RED));
    let glass = LayerDraw {
        backdrop: Some(backdrop(Rect::new(20.0, 0.0, 40.0, 40.0), 8.0)),
        ..LayerDraw::new(&empty)
    };
    let frame = [LayerDraw::new(&scene), glass];

    let first = canvas.frame(&frame);
    assert_eq!((first.backdrop_passes, first.backdrops_reused), (1, 0));
    let blurred = canvas.image()?.pixel(40, 20);
    assert!(
        blurred[0] > 20 && blurred[0] < 235,
        "a blurred edge: {blurred:?}"
    );

    let again = canvas.frame(&frame);
    assert_eq!(
        (
            again.backdrop_passes,
            again.backdrop_blurs,
            again.backdrops_reused
        ),
        (0, 0, 1)
    );
    assert_eq!(
        canvas.image()?.pixel(40, 20),
        blurred,
        "the reused texture gives the same picture"
    );

    let on_top = canvas.frame(&[LayerDraw::new(&scene), glass, LayerDraw::new(&above)]);
    assert_eq!(
        (on_top.backdrop_passes, on_top.backdrops_reused),
        (0, 1),
        "drawing above it changes nothing behind"
    );
    assert_eq!(canvas.image()?.pixel(15, 15), RED);

    let changed = canvas.frame(&[
        LayerDraw {
            opacity: 0.5,
            ..LayerDraw::new(&scene)
        },
        glass,
    ]);
    assert_eq!(
        (changed.backdrop_passes, changed.backdrops_reused),
        (1, 0),
        "a change behind re-blurs"
    );
    Ok(())
}

#[test]
fn backdrop_effects_follow_the_layer_transform() -> Result<(), GpuError> {
    let (scene, empty) = (quads([full(Color::WHITE)]), DrawList::new());
    let tint = Glass::frosted(0.0).tint(Color::RED);
    let glass = EffectDraw::new(&tint, Rect::new(40.0, 40.0, 20.0, 20.0));
    let turned = Affine::rotate(FRAC_PI_4).about(Point::new(50.0, 50.0));
    let layers = [
        LayerDraw::new(&scene),
        LayerDraw {
            backdrop: Some(glass),
            transform: turned,
            ..LayerDraw::new(&empty)
        },
    ];
    let img = render_layers(100, &layers)?;
    assert_near(img.pixel(50, 50), RED, "the middle is tinted");
    assert_near(
        img.pixel(50, 38),
        RED,
        "a diamond reaches further up than the square did",
    );
    assert_near(
        img.pixel(41, 41),
        WHITE,
        "and its corners no longer cover the square's",
    );
    Ok(())
}

#[test]
fn backing_primitives_take_fill_and_radii_from_the_layer() -> Result<(), GpuError> {
    let mut list = DrawList::new();
    list.push_shadow(Shadow {
        backing: true,
        ..Shadow::new(Rect::new(20.0, 20.0, 60.0, 60.0), Color::BLACK, 0.0)
    });
    list.push_quad(Quad::backing(Rect::new(30.0, 30.0, 40.0, 40.0)));
    let layer = LayerDraw {
        fill: Color::BLUE.premultiplied(),
        radii: Corners::all(20.0),
        ..LayerDraw::new(&list)
    };
    let img = render_layers(100, &[layer])?;
    assert_near(img.pixel(50, 50), BLUE, "filled by the layer");
    assert_near(
        img.pixel(32, 32),
        [0, 0, 0, 255],
        "the quad's corner is cut, showing the shadow",
    );
    assert_near(
        img.pixel(22, 22),
        CLEAR,
        "and the shadow is rounded the same way",
    );
    Ok(())
}

#[test]
fn images_are_sampled_into_their_bounds_between_other_primitives() -> Result<(), Box<dyn Error>> {
    let mut list = quads([full(Color::WHITE)]);
    list.push_image(ImageQuad::new(
        Rect::new(10.0, 10.0, 40.0, 40.0),
        checker()?,
    ));
    list.push_quad(Quad::new(Rect::new(40.0, 40.0, 20.0, 20.0), Color::BLUE));
    let mut canvas = Canvas::new(100, 1.0, Color::TRANSPARENT)?;
    let stats = canvas.frame(&[LayerDraw::new(&list)]);
    let img = canvas.image()?;
    assert_near(img.pixel(12, 12), RED, "top-left texel");
    assert_near(img.pixel(48, 12), BLUE, "top-right texel");
    assert_near(img.pixel(12, 48), BLUE, "bottom-left texel");
    assert_near(
        img.pixel(30, 12),
        [121, 0, 134, 255],
        "texels are filtered between their centers",
    );
    assert_near(img.pixel(5, 5), WHITE, "quad drawn before the image");
    assert_near(
        img.pixel(45, 45),
        BLUE,
        "quad drawn after the image covers it",
    );
    assert_near(img.pixel(55, 30), WHITE, "outside the image");
    assert_eq!(
        (stats.images, stats.draw_calls),
        (1, 3),
        "one draw call per run of same-texture primitives"
    );
    assert_eq!(stats.image_bytes_uploaded, 16);
    let again = canvas.frame(&[LayerDraw::new(&list)]);
    assert_eq!(
        again.image_bytes_uploaded, 0,
        "an uploaded image stays on the GPU"
    );
    Ok(())
}

#[test]
fn images_take_rounded_corners_clip_source_rect_and_tint() -> Result<(), Box<dyn Error>> {
    let solid = ImageData::rgba8(1, 1, vec![255, 0, 0, 255])?;
    let mut list = DrawList::new();
    let rounded = ImageQuad::new(Rect::new(0.0, 0.0, 40.0, 40.0), solid.clone());
    list.push_image(ImageQuad {
        corner_radii: Corners::all(20.0),
        ..rounded
    });
    let clipped = ImageQuad::new(Rect::new(50.0, 0.0, 40.0, 40.0), solid.clone());
    list.push_image(ImageQuad {
        clip: Rect::new(50.0, 0.0, 20.0, 40.0),
        ..clipped
    });
    let quarter = ImageQuad::new(Rect::new(0.0, 50.0, 40.0, 40.0), checker()?);
    list.push_image(ImageQuad {
        source: Rect::new(0.5, 0.0, 0.5, 0.5),
        ..quarter
    });
    let faded = ImageQuad::new(Rect::new(50.0, 50.0, 40.0, 40.0), solid);
    list.push_image(ImageQuad {
        tint: Color::new(1.0, 1.0, 1.0, 0.5),
        ..faded
    });
    let img = render(1.0, 100, Color::TRANSPARENT, &list)?;
    assert_near(img.pixel(20, 20), RED, "center of the rounded image");
    assert_near(img.pixel(1, 1), CLEAR, "corner is cut");
    assert_near(img.pixel(60, 20), RED, "inside the clip");
    assert_near(img.pixel(80, 20), CLEAR, "outside the clip");
    assert_near(img.pixel(5, 55), BLUE, "source rect selects one texel");
    assert_near(
        img.pixel(35, 85),
        BLUE,
        "source rect is clamped at its edges",
    );
    assert_near(
        img.pixel(70, 70),
        [128, 0, 0, 128],
        "tint alpha fades the image",
    );
    Ok(())
}

#[test]
fn alpha_images_are_masks_filled_with_the_tint_and_straight_alpha_is_premultiplied()
-> Result<(), Box<dyn Error>> {
    let mask = ImageData::alpha8(2, 1, vec![255, 0])?;
    let translucent = ImageData::rgba8(1, 1, vec![0, 255, 0, 128])?;
    let mut list = quads([full(Color::WHITE)]);
    let masked = ImageQuad::new(Rect::new(0.0, 0.0, 40.0, 20.0), mask);
    list.push_image(ImageQuad {
        tint: Color::BLUE,
        ..masked
    });
    list.push_image(ImageQuad::new(
        Rect::new(0.0, 50.0, 20.0, 20.0),
        translucent,
    ));
    let img = render(1.0, 100, Color::TRANSPARENT, &list)?;
    assert_near(img.pixel(5, 10), BLUE, "covered texel takes the tint");
    assert_near(
        img.pixel(35, 10),
        WHITE,
        "empty texel leaves the background",
    );
    assert_near(
        img.pixel(10, 60),
        [127, 255, 127, 255],
        "half-transparent green over white",
    );
    Ok(())
}
fn checker() -> Result<ImageData, Box<dyn Error>> {
    let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255]);
    Ok(ImageData::rgba8(2, 2, [red, blue, blue, red].concat())?)
}

fn render(scale: f32, size: u32, clear: Color, list: &DrawList) -> Result<Image, GpuError> {
    let mut canvas = Canvas::new(size, scale, clear)?;
    canvas.frame(&[LayerDraw::new(list)]);
    canvas.image()
}

fn square(color: Color) -> DrawList {
    quads([Quad::new(Rect::new(10.0, 10.0, 20.0, 20.0), color)])
}

const INSTANCE_BYTES: u64 = 112;
