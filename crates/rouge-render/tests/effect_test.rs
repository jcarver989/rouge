mod common;

use common::{RED, WHITE, backdrop, full, over, quads, render_layers, split_scene};
use rouge_render::headless::assert_near;
use rouge_render::{Color, Corners, EffectDraw, Glass, GpuError, LayerDraw, Point, Quad, Rect};

#[test]
fn backdrop_blur_softens_what_is_behind_it_only_inside_its_shape() -> Result<(), GpuError> {
    let glass = backdrop(Rect::new(0.0, 0.0, 100.0, 50.0), 16.0);
    let img = over(split_scene(), glass)?;

    let row = |y: u32| (0..100).map(|x| img.pixel(x, y)[0]).collect::<Vec<u8>>();
    let blurred = row(25);
    assert!(
        blurred[20] < 40,
        "far from the edge stays dark: {}",
        blurred[20]
    );
    assert!(
        blurred[80] > 215,
        "far from the edge stays light: {}",
        blurred[80]
    );
    assert!(
        (100..156).contains(&blurred[50]),
        "the edge becomes mid-gray: {}",
        blurred[50]
    );
    assert!(
        blurred[44] > 20 && blurred[56] < 235,
        "the transition is wide"
    );
    assert!(
        blurred.windows(2).all(|w| w[0] <= w[1].saturating_add(1)),
        "smooth ramp: {blurred:?}"
    );
    assert_eq!(img.pixel(30, 25)[3], 255, "the result is opaque");

    let sharp = row(75);
    assert_eq!(
        (sharp[49], sharp[50]),
        (0, 255),
        "outside the shape the scene is untouched"
    );
    Ok(())
}

#[test]
fn larger_blur_radius_spreads_further() -> Result<(), GpuError> {
    let at = |blur: f32| {
        let glass = backdrop(Rect::new(0.0, 0.0, 100.0, 100.0), blur);
        Ok::<_, GpuError>(over(split_scene(), glass)?.pixel(38, 50)[0])
    };
    let (small, large) = (at(4.0)?, at(32.0)?);
    assert!(
        small < 10,
        "a small blur has not reached 12px from the edge: {small}"
    );
    assert!(large > small + 20, "a large blur has: {large} vs {small}");
    Ok(())
}

#[test]
fn backdrop_tint_and_rounded_shape() -> Result<(), GpuError> {
    let glass = EffectDraw {
        corner_radii: Corners::all(40.0),
        ..EffectDraw::new(
            &Glass::frosted(8.0).tint(Color::RED.with_alpha(0.5)),
            Rect::new(10.0, 10.0, 80.0, 80.0),
        )
    };
    let img = over(quads([full(Color::WHITE)]), glass)?;
    assert_near(
        img.pixel(50, 50),
        [255, 127, 127, 255],
        "white backdrop tinted half red",
    );
    assert_near(img.pixel(13, 13), WHITE, "outside the rounded corner");
    assert_near(img.pixel(5, 50), WHITE, "outside the bounds");
    Ok(())
}

#[test]
fn layer_content_draws_sharp_on_top_of_its_backdrop() -> Result<(), GpuError> {
    let scene = split_scene();
    let content = quads([Quad::new(Rect::new(40.0, 40.0, 20.0, 20.0), Color::RED)]);
    let glass = backdrop(Rect::new(0.0, 0.0, 100.0, 100.0), 16.0);
    let img = render_layers(100, |canvas| {
        vec![
            LayerDraw::new(canvas.add(scene)),
            LayerDraw {
                backdrop: Some(glass),
                ..LayerDraw::new(canvas.add(content))
            },
        ]
    })?;
    assert_near(img.pixel(50, 50), RED, "content is above the glass");
    assert_near(img.pixel(40, 40), RED, "and has crisp edges");
    assert_ne!(img.pixel(39, 39), RED);
    Ok(())
}

#[test]
fn refraction_and_highlight_affect_only_the_rim() -> Result<(), GpuError> {
    let draw = |bounds: Rect, glass: Glass| {
        let backdrop = EffectDraw {
            corner_radii: Corners::all(20.0),
            ..EffectDraw::new(&glass, bounds)
        };
        over(split_scene(), backdrop)
    };
    let bounds = Rect::new(10.0, 10.0, 80.0, 80.0);
    let plain = Glass::frosted(4.0).edge_width(12.0);
    let glassy = plain.refraction(1.0).dispersion(0.3).highlight(0.6);
    let (a, b) = (draw(bounds, plain)?, draw(bounds, glassy)?);

    for (x, y) in [(30, 50), (70, 50), (30, 30), (70, 70)] {
        assert_near(
            b.pixel(x, y),
            a.pixel(x, y),
            "the flat middle is unaffected",
        );
    }
    let rim = b.pixel(10, 50);
    assert!(
        rim[0] > a.pixel(10, 50)[0] + 30,
        "a thin highlight on the lit rim: {rim:?}"
    );

    let over_edge = draw(Rect::new(45.0, 10.0, 50.0, 80.0), glassy)?;
    let fringe = (45..58).any(|x| {
        let [r, _, bl, _] = over_edge.pixel(x, 50);
        (i32::from(r) - i32::from(bl)).abs() > 12
    });
    assert!(
        fringe,
        "a rim bending light across the black/white edge separates the channels"
    );
    Ok(())
}

#[test]
fn saturation_scales_how_colorful_the_backdrop_shows_through() -> Result<(), GpuError> {
    let chroma = |glass: Glass| {
        let scene = quads([full(Color::new(0.8, 0.4, 0.4, 1.0))]);
        let img = over(
            scene,
            EffectDraw::new(&glass, Rect::new(10.0, 10.0, 80.0, 80.0)),
        )?;
        let [r, g, ..] = img.pixel(50, 50);
        Ok::<_, GpuError>(i32::from(r) - i32::from(g))
    };
    let plain = chroma(Glass::frosted(0.0))?;

    assert_eq!(chroma(Glass::frosted(0.0).saturation(0.0))?, 0, "gray");
    assert!(
        chroma(Glass::frosted(0.0).saturation(1.5))? > plain + 30,
        "more vivid than the backdrop itself"
    );
    Ok(())
}

#[test]
fn a_light_brightens_the_rim_nearest_to_it_and_leaves_the_far_side_alone() -> Result<(), GpuError> {
    let draw = |glass: Glass| {
        let backdrop = EffectDraw {
            corner_radii: Corners::all(20.0),
            ..EffectDraw::new(&glass, Rect::new(10.0, 10.0, 80.0, 80.0))
        };
        over(quads([full(Color::BLACK)]), backdrop)
    };
    let dark = Glass::frosted(0.0).edge_width(12.0);
    let (unlit, lit) = (
        draw(dark)?,
        draw(dark.light(Point::new(0.0, 40.0), 1.0, 20.0))?,
    );

    let near = lit.pixel(10, 50);
    assert!(
        near[0] > unlit.pixel(10, 50)[0] + 60,
        "the rim beside the light catches it: {near:?}"
    );
    assert_near(
        lit.pixel(50, 50),
        unlit.pixel(50, 50),
        "the light has faded by the middle",
    );
    assert_near(
        lit.pixel(89, 50),
        unlit.pixel(89, 50),
        "and never reaches the far rim",
    );
    Ok(())
}

#[test]
fn an_overhead_rim_light_lights_the_top_edge_and_leaves_the_bottom_unfilled() -> Result<(), GpuError>
{
    let draw = |glass: Glass| {
        let backdrop = EffectDraw {
            corner_radii: Corners::all(20.0),
            ..EffectDraw::new(&glass, Rect::new(10.0, 10.0, 80.0, 80.0))
        };
        over(quads([full(Color::BLACK)]), backdrop)
    };
    let lit = Glass::frosted(0.0).edge_width(1.5).highlight(1.0);
    let (diagonal, overhead) = (draw(lit)?, draw(lit.rim_light(0.0, 0.0))?);

    let (top, side, bottom) = (
        overhead.pixel(50, 10)[0],
        overhead.pixel(10, 50)[0],
        overhead.pixel(50, 89)[0],
    );
    assert!(
        top > side + 60,
        "the top edge catches the light: {top} vs {side}"
    );
    assert!(
        top > bottom + 60,
        "the bottom edge stays dark: {top} vs {bottom}"
    );
    let filled = diagonal.pixel(50, 89)[0];
    assert!(
        filled > bottom + 30,
        "without fill the bottom is darker than the default: {filled} vs {bottom}"
    );
    Ok(())
}

#[test]
fn shadow_falls_outside_the_glass_and_never_darkens_it() -> Result<(), GpuError> {
    let draw = |glass: Glass| {
        let bounds = Rect::new(30.0, 30.0, 40.0, 40.0);
        let backdrop = EffectDraw {
            corner_radii: Corners::all(8.0),
            ..EffectDraw::new(&glass, bounds)
        };
        over(quads([full(Color::WHITE)]), backdrop)
    };
    let (plain, shadowed) = (
        draw(Glass::frosted(2.0))?,
        draw(Glass::frosted(2.0).shadow(0.5))?,
    );

    assert_near(plain.pixel(50, 74), WHITE, "no shadow unless asked for");
    let below = shadowed.pixel(50, 74);
    assert!(below[0] < 200, "shadow under the bottom edge: {below:?}");
    assert_near(
        shadowed.pixel(50, 50),
        WHITE,
        "the glass itself stays clear",
    );
    assert_near(shadowed.pixel(50, 99), WHITE, "the shadow fades out");
    Ok(())
}
