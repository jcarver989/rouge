mod common;

use common::{
    BLUE, CHILD, RED, StageBuilder, TestResult, WHITE, backdrop, full, over, quads, render_layers,
    split_scene, square,
};
use rouge_render::headless::assert_near;
use rouge_render::{
    BlendMode, Color, ColorMatrix, Corners, Effect, EffectDraw, EffectSlot, Glass, GpuError,
    Inputs, LayerDraw, LayerProp, Mask, Point, Pose, Quad, Rect,
};

#[test]
fn backdrop_blur_softens_what_is_behind_it_only_inside_its_shape() -> Result<(), GpuError> {
    let glass = backdrop(Rect::new(0.0, 0.0, 100.0, 50.0), 16.0);
    let img = over(&split_scene(), glass)?;

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
        Ok::<_, GpuError>(over(&split_scene(), glass)?.pixel(38, 50)[0])
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
    let img = over(&quads([full(Color::WHITE)]), glass)?;
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
    let img = render_layers(
        100,
        &[
            LayerDraw::new(&scene),
            LayerDraw {
                backdrop: Some(glass),
                ..LayerDraw::new(&content)
            },
        ],
    )?;
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
        over(&split_scene(), backdrop)
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
fn shadow_falls_outside_the_glass_and_never_darkens_it() -> Result<(), GpuError> {
    let draw = |glass: Glass| {
        let bounds = Rect::new(30.0, 30.0, 40.0, 40.0);
        let backdrop = EffectDraw {
            corner_radii: Corners::all(8.0),
            ..EffectDraw::new(&glass, bounds)
        };
        over(&quads([full(Color::WHITE)]), backdrop)
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

#[test]
fn effects_chain_in_order_before_the_mask() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(square())
        .at(40.0, 40.0)
        .build()?;
    stage.effect(
        CHILD,
        EffectSlot::Content(0),
        &ColorMatrix::grayscale(),
        false,
    );
    stage.effect(CHILD, EffectSlot::Content(1), &ColorMatrix::invert(), false);
    let fade = Mask::linear_gradient(Point::new(0.0, 0.0), Point::new(1.0, 0.0));
    stage.effect(CHILD, EffectSlot::Mask, &fade, false);
    let stats = stage.draw();
    assert_eq!(
        (stats.groups, stats.group_passes),
        (3, 3),
        "one stage per effect"
    );
    let img = stage.image()?;
    assert_near(
        img.pixel(40, 50),
        [203, 203, 203, 255],
        "gray then inverted, nearly opaque on the left",
    );
    assert_near(
        img.pixel(59, 50),
        [254, 254, 254, 255],
        "faded out on the right",
    );

    stage.effect_off(CHILD, EffectSlot::Content(1));
    stage.effect_off(CHILD, EffectSlot::Mask);
    stage.draw();
    assert_near(
        stage.image()?.pixel(59, 50),
        [54, 54, 54, 255],
        "removing a stage restores the rest",
    );
    Ok(())
}

#[test]
fn masks_can_clip_to_the_rounded_shape_or_fade_radially() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(square())
        .at(40.0, 40.0)
        .build()?;
    stage.pose(CHILD, Pose::new().rounded(10.0), false);
    stage.effect(CHILD, EffectSlot::Mask, &Mask::shape(), false);
    stage.draw();
    assert_eq!(
        (stage.image()?.pixel(50, 50), stage.image()?.pixel(41, 41)),
        (RED, WHITE)
    );

    let spot = Mask::radial_gradient(Point::new(0.5, 0.5), 0.5)
        .stops(0.5, 1.0)
        .invert();
    stage.effect(CHILD, EffectSlot::Mask, &spot, false);
    stage.draw();
    assert_eq!(
        (stage.image()?.pixel(50, 50), stage.image()?.pixel(41, 41)),
        (WHITE, RED),
        "inverted"
    );
    Ok(())
}

#[test]
fn blend_modes_mix_with_what_is_behind() -> TestResult {
    let behind = quads([
        Quad::new(Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE),
        Quad::new(
            Rect::new(50.0, 0.0, 50.0, 100.0),
            Color::new(0.5, 0.5, 0.5, 1.0),
        ),
    ]);
    let top = quads([Quad::new(
        Rect::new(0.0, 0.0, 20.0, 20.0),
        Color::new(1.0, 0.5, 0.0, 1.0),
    )]);
    let mut stage = StageBuilder::default()
        .background(behind)
        .child(top)
        .at(40.0, 40.0)
        .build()?;
    let mut mixed = |mode: BlendMode| {
        stage.effect(CHILD, EffectSlot::Blend, &mode, false);
        let stats = stage.draw();
        let img = stage.image()?;
        Ok::<_, GpuError>(((img.pixel(45, 50), img.pixel(55, 50)), stats))
    };
    let ((on_white, on_gray), stats) = mixed(BlendMode::Multiply)?;
    assert_eq!(
        stats.backdrop_passes, 1,
        "reading the backdrop breaks the pass once"
    );
    assert_near(
        on_white,
        [255, 128, 0, 255],
        "multiplying by white changes nothing",
    );
    assert_near(on_gray, [128, 64, 0, 255], "multiplying by gray halves it");
    let ((on_white, on_gray), stats) = mixed(BlendMode::Difference)?;
    assert_eq!(
        (stats.backdrop_passes, stats.backdrops_reused),
        (0, 1),
        "the copy behind is still good"
    );
    assert_near(
        on_white,
        [0, 128, 255, 255],
        "difference from white inverts",
    );
    assert_near(on_gray, [128, 0, 128, 255], "difference from gray");
    let ((on_white, on_gray), _) = mixed(BlendMode::Screen)?;
    assert_near(
        on_white,
        [255, 255, 255, 255],
        "screen on white stays white",
    );
    assert_near(on_gray, [255, 191, 128, 255], "screen on gray lightens");
    Ok(())
}

#[test]
fn custom_effects_supply_their_own_shader_and_uniforms() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(square())
        .at(40.0, 40.0)
        .build()?;
    stage.effect(
        CHILD,
        EffectSlot::Content(0),
        &Stripes {
            period: 10.0,
            tint: Color::BLUE,
        },
        false,
    );
    stage.draw();
    let img = stage.image()?;
    assert_eq!(img.pixel(42, 50), RED, "the first stripe keeps the content");
    assert_eq!(
        img.pixel(47, 50),
        BLUE,
        "the second takes the uniform color"
    );
    assert_eq!(img.pixel(52, 50), RED);
    assert_eq!(img.pixel(47, 30), WHITE, "nothing outside the content");

    stage.set(CHILD, LayerProp::TranslateX, 5.0);
    stage.draw();
    assert_eq!(
        stage.image()?.pixel(47, 50),
        RED,
        "local coordinates follow the layer"
    );
    Ok(())
}

struct Stripes {
    period: f32,
    tint: Color,
}

impl Effect for Stripes {
    const WGSL: &'static str = "
        fn effect(in: EffectInput) -> vec4<f32> {
            let content = sample_content(in.local);
            let odd = floor(in.local.x * 2.0 / param(0).x) % 2.0;
            return mix(content, param(1) * content.a, odd);
        }
    ";
    const INPUTS: Inputs = Inputs::CONTENT;

    fn uniforms(&self) -> Vec<f32> {
        let mut uniforms = vec![self.period, 0.0, 0.0, 0.0];
        uniforms.extend(self.tint.to_array());
        uniforms
    }
}
