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
    let img = render(1.0, 100, Color::TRANSPARENT, list)?;
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
    let img = render(2.0, 100, Color::WHITE, list)?;
    assert_near(img.pixel(20, 20), RED, "device origin is doubled");
    assert_near(img.pixel(59, 59), RED, "device extent is doubled");
    assert_near(img.pixel(19, 19), WHITE, "outside");
    assert_near(img.pixel(60, 60), WHITE, "outside");
    Ok(())
}

#[test]
fn stacked_quads_meet_without_seams_at_fractional_scales() -> Result<(), GpuError> {
    let row = |index: u8, clipped: bool| {
        let bounds = Rect::new(0.0, f32::from(index) * 20.0, 100.0, 20.0);
        Quad {
            clip: if clipped { bounds } else { Rect::EVERYTHING },
            ..Quad::new(bounds, Color::RED)
        }
    };
    for clipped in [false, true] {
        let img = render(
            1.875,
            160,
            Color::BLACK,
            quads((0..4).map(|index| row(index, clipped))),
        )?;
        for y in 0..150 {
            assert_near(
                img.pixel(10, y),
                RED,
                &format!("row pixel {y}, clipped: {clipped}"),
            );
        }
        assert_near(img.pixel(10, 150), [0, 0, 0, 255], "below the last row");
    }
    Ok(())
}

#[test]
fn rounded_corners_are_cut_and_antialiased() -> Result<(), GpuError> {
    let list = quads([Quad {
        corner_radii: Corners::all(50.0),
        ..full(Color::RED)
    }]);
    let img = render(1.0, 100, Color::TRANSPARENT, list)?;
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
        quads([Quad {
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
    let img = render(1.0, 100, Color::TRANSPARENT, list)?;
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
fn hairline_borders_are_whole_device_pixels_wherever_they_sit() -> Result<(), GpuError> {
    let boxed = |top: f32| Quad {
        border_color: Color::BLUE,
        border_widths: Edges {
            top: 1.0,
            ..Edges::ZERO
        },
        ..Quad::new(Rect::new(0.0, top, 100.0, 10.0), Color::RED)
    };
    let img = render(
        1.875,
        160,
        Color::BLACK,
        quads([0.0, 12.0, 24.0, 36.0, 48.0].map(boxed)),
    )?;
    assert_eq!(
        runs(column(&img), BLUE),
        vec![2; 5],
        "every border is 2 device pixels"
    );
    Ok(())
}

#[test]
fn lines_are_whole_device_pixels_wherever_they_sit() -> Result<(), GpuError> {
    let offsets = [0.0, 4.0, 8.0, 12.0, 16.0];
    let horizontal = offsets.map(|y| Quad::line(Rect::new(0.0, y, 100.0, 1.0), Color::BLUE));
    let vertical = offsets.map(|x| Quad::line(Rect::new(x, 0.0, 1.0, 100.0), Color::BLUE));
    let img = render(1.875, 160, Color::BLACK, quads(horizontal))?;
    assert_eq!(runs(column(&img), BLUE), vec![2; 5], "horizontal lines");
    let img = render(1.875, 160, Color::BLACK, quads(vertical))?;
    assert_eq!(runs(row(&img), BLUE), vec![2; 5], "vertical lines");
    Ok(())
}

#[test]
fn quads_keep_their_device_size_wherever_they_sit() -> Result<(), GpuError> {
    let strip = |top: f32| Quad::new(Rect::new(0.0, top, 100.0, 8.0), Color::RED);
    let img = render(
        1.875,
        160,
        Color::BLACK,
        quads([4.0, 16.0, 28.0, 40.0].map(strip)),
    )?;
    assert_eq!(
        runs(column(&img), RED),
        vec![15; 4],
        "8 logical pixels are 15 device pixels"
    );
    Ok(())
}

#[test]
fn translucent_quads_blend_in_gamma_space() -> Result<(), GpuError> {
    let list = quads([Quad::new(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Color::BLACK.with_alpha(0.5),
    )]);
    let img = render(1.0, 10, Color::WHITE, list)?;
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
    let img = render(1.0, 100, Color::TRANSPARENT, list)?;
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
    let img = render(1.0, 10, Color::TRANSPARENT, list)?;
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
    let img = render(1.0, 120, Color::TRANSPARENT, list)?;
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
        let key = canvas.add(list);
        canvas.frame(&[LayerDraw::new(key)]);
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
    let key = canvas.add(list);
    canvas.frame(&[LayerDraw::new(key)]);

    let second = canvas
        .atlas()
        .insert(4, 4, &[255; 16])
        .ok_or("atlas has room")?;
    let list = canvas.list(key)?;
    list.clear();
    list.push_glyph(Glyph {
        origin: Point::ZERO,
        clip: Rect::new(0.0, 0.0, 2.0, 4.0),
        color: Color::BLUE,
        mask: second,
    });
    canvas.frame(&[LayerDraw::new(key)]);
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
        render_layers(100, |canvas| {
            vec![LayerDraw {
                transform,
                ..LayerDraw::new(canvas.add(square(Color::RED)))
            }]
        })
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
    let img = render_layers(40, |canvas| {
        let faded = LayerDraw {
            transform: Affine::translate(10.0, 0.0),
            opacity: 0.5,
            ..LayerDraw::new(canvas.add(square(Color::BLUE)))
        };
        vec![LayerDraw::new(canvas.add(square(Color::RED))), faded]
    })?;
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

    let hidden = render_layers(40, |canvas| {
        vec![LayerDraw {
            opacity: 0.0,
            ..LayerDraw::new(canvas.add(square(Color::RED)))
        }]
    })?;
    assert_near(
        hidden.pixel(20, 20),
        CLEAR,
        "fully transparent layer draws nothing",
    );
    Ok(())
}

#[test]
fn unchanged_lists_are_not_uploaded_again() -> Result<(), Box<dyn Error>> {
    let mut canvas = Canvas::new(64, 1.0, Color::WHITE)?;
    let a = canvas.add(square(Color::RED));
    let b = canvas.add(square(Color::BLUE));
    let frame = |canvas: &mut Canvas, x: f32| {
        canvas.frame(&[
            LayerDraw::new(a),
            LayerDraw {
                transform: Affine::translate(x, 0.0),
                ..LayerDraw::new(b)
            },
        ])
    };

    let first = frame(&mut canvas, 0.0);
    assert_eq!(
        first.instance_bytes_uploaded,
        2 * INSTANCE_BYTES,
        "both lists uploaded once"
    );
    assert_eq!((first.layers, first.draw_calls), (2, 2));
    for x in 1..10 {
        assert_eq!(
            frame(&mut canvas, x as f32).instance_bytes_uploaded,
            0,
            "transforms send no instances"
        );
    }

    let list = canvas.list(a)?;
    list.clear();
    list.push_quad(Quad::new(Rect::new(0.0, 0.0, 5.0, 5.0), Color::RED));
    assert_eq!(
        frame(&mut canvas, 0.0).instance_bytes_uploaded,
        INSTANCE_BYTES,
        "only the rebuilt list"
    );
    assert_eq!(frame(&mut canvas, 0.0).instance_bytes_uploaded, 0);

    let list = canvas.list(a)?;
    list.clear();
    for i in 0..1000 {
        let at = Rect::new((i % 50) as f32, (40 + i / 50) as f32, 1.0, 1.0);
        list.push_quad(Quad::new(at, Color::RED));
    }
    assert_eq!(
        frame(&mut canvas, 0.0).instance_bytes_uploaded,
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
    let img = render_layers(20, |canvas| {
        let list = canvas.add(quads([Quad::new(
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::WHITE,
        )]));
        Vec::from_fn(200, |i| LayerDraw {
            transform: Affine::translate((i % 20) as f32, (i / 20) as f32),
            ..LayerDraw::new(list)
        })
    })?;
    assert_near(img.pixel(0, 0), WHITE, "first layer");
    assert_near(img.pixel(19, 9), WHITE, "last layer");
    assert_near(img.pixel(0, 10), CLEAR, "beyond the last layer");
    Ok(())
}

#[test]
fn layers_spanning_many_uniform_pages_each_keep_their_own_transform() -> Result<(), GpuError> {
    let img = render_layers(40, |canvas| {
        let list = canvas.add(quads([Quad::new(
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::WHITE,
        )]));
        Vec::from_fn(1600, |i| LayerDraw {
            transform: Affine::translate((i % 40) as f32, (i / 40) as f32),
            ..LayerDraw::new(list)
        })
    })?;
    for (x, y) in (0..40).flat_map(|y| (0..40).map(move |x| (x, y))) {
        assert_near(img.pixel(x, y), WHITE, "every layer lands on its own pixel");
    }
    Ok(())
}

#[test]
fn lists_uploaded_before_the_instance_buffer_grows_still_draw() -> Result<(), GpuError> {
    let mut canvas = Canvas::new(64, 1.0, Color::WHITE)?;
    let early = canvas.add(square(Color::BLUE));
    canvas.frame(&[LayerDraw::new(early)]);

    let mut late = DrawList::new();
    for i in 0..2000 {
        let at = Rect::new((i % 50) as f32, (40 + i / 50 % 20) as f32, 1.0, 1.0);
        late.push_quad(Quad::new(at, Color::RED));
    }
    let late = canvas.add(late);
    let stats = canvas.frame(&[LayerDraw::new(early), LayerDraw::new(late)]);
    assert_eq!(
        stats.instance_bytes_uploaded,
        2000 * INSTANCE_BYTES,
        "only the new list is uploaded"
    );
    let img = canvas.image()?;
    assert_near(
        img.pixel(20, 20),
        BLUE,
        "the earlier list survives the growth",
    );
    assert_near(img.pixel(49, 59), RED, "the new list is drawn");
    Ok(())
}

#[test]
fn backdrop_passes_are_counted_and_absent_without_effects() -> Result<(), GpuError> {
    let mut canvas = Canvas::new(64, 1.0, Color::WHITE)?;
    let scene = canvas.add(split_scene());
    let glass = LayerDraw {
        backdrop: Some(backdrop(Rect::new(0.0, 0.0, 20.0, 20.0), 8.0)),
        ..LayerDraw::default()
    };

    let plain = canvas.frame(&[LayerDraw::new(scene)]);
    assert_eq!(plain.backdrop_passes, 0, "no effects, no offscreen work");
    let two = canvas.frame(&[LayerDraw::new(scene), glass, glass]);
    assert_eq!(two.backdrop_passes, 2);
    let hidden = canvas.frame(&[
        LayerDraw::new(scene),
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
    let scene = canvas.add(split_scene());
    let glass = |x: f32, y: f32| LayerDraw {
        backdrop: Some(backdrop(Rect::new(x, y, 16.0, 16.0), 2.0)),
        ..LayerDraw::default()
    };

    let stats = canvas.frame(&[LayerDraw::new(scene), glass(0.0, 0.0), glass(40.0, 40.0)]);
    assert_eq!(
        (stats.backdrop_passes, stats.backdrop_blurs),
        (1, 2),
        "one break serves both"
    );

    let stats = canvas.frame(&[LayerDraw::new(scene), glass(4.0, 4.0), glass(14.0, 14.0)]);
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
    let (scene, above) = (canvas.add(split_scene()), canvas.add(square(Color::RED)));
    let glass = LayerDraw {
        backdrop: Some(backdrop(Rect::new(20.0, 0.0, 40.0, 40.0), 8.0)),
        ..LayerDraw::default()
    };
    let frame = [LayerDraw::new(scene), glass];

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

    let on_top = canvas.frame(&[LayerDraw::new(scene), glass, LayerDraw::new(above)]);
    assert_eq!(
        (on_top.backdrop_passes, on_top.backdrops_reused),
        (0, 1),
        "drawing above it changes nothing behind"
    );
    assert_eq!(canvas.image()?.pixel(15, 15), RED);

    let changed = canvas.frame(&[
        LayerDraw {
            opacity: 0.5,
            ..LayerDraw::new(scene)
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
    let tint = Glass::frosted(0.0).tint(Color::RED);
    let glass = EffectDraw::new(&tint, Rect::new(40.0, 40.0, 20.0, 20.0));
    let turned = Affine::rotate(FRAC_PI_4).about(Point::new(50.0, 50.0));
    let img = render_layers(100, |canvas| {
        vec![
            LayerDraw::new(canvas.add(quads([full(Color::WHITE)]))),
            LayerDraw {
                backdrop: Some(glass),
                transform: turned,
                ..LayerDraw::default()
            },
        ]
    })?;
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
    let img = render_layers(100, |canvas| {
        vec![LayerDraw {
            fill: Color::BLUE.premultiplied(),
            radii: Corners::all(20.0),
            ..LayerDraw::new(canvas.add(list))
        }]
    })?;
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
    let list = canvas.add(list);
    let stats = canvas.frame(&[LayerDraw::new(list)]);
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
    let again = canvas.frame(&[LayerDraw::new(list)]);
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
    let img = render(1.0, 100, Color::TRANSPARENT, list)?;
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
    let img = render(1.0, 100, Color::TRANSPARENT, list)?;
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

fn render(scale: f32, size: u32, clear: Color, list: DrawList) -> Result<Image, GpuError> {
    let mut canvas = Canvas::new(size, scale, clear)?;
    let list = canvas.add(list);
    canvas.frame(&[LayerDraw::new(list)]);
    canvas.image()
}

fn column(img: &Image) -> Vec<[u8; 4]> {
    (0..img.height).map(|y| img.pixel(10, y)).collect()
}

fn row(img: &Image) -> Vec<[u8; 4]> {
    (0..img.width).map(|x| img.pixel(x, 10)).collect()
}

fn runs(pixels: Vec<[u8; 4]>, color: [u8; 4]) -> Vec<usize> {
    let mut runs = Vec::new();
    let mut length = 0;
    for (at, pixel) in pixels.into_iter().enumerate() {
        assert!(
            pixel == color || pixel == [0, 0, 0, 255] || pixel == RED,
            "pixel {at} is a blend: {pixel:?}"
        );
        if pixel == color {
            length += 1;
        } else if length > 0 {
            runs.push(length);
            length = 0;
        }
    }
    runs
}

fn square(color: Color) -> DrawList {
    quads([Quad::new(Rect::new(10.0, 10.0, 20.0, 20.0), color)])
}

const INSTANCE_BYTES: u64 = 84;
