mod common;

use common::{Stage, StageBuilder, background, quads, settled, square};
use rouge_core::Corners;
use rouge_core::testing::{BLUE, LINEAR, RED, TestResult, WHITE, assert_near};
use rouge_core::{
    Animation, BlendMode, Blur, Color, ColorMatrix, Decay, EffectSlot, Inputs, Keyframes,
    LayerProp, LayoutTransition, Mask, MotionId, Point, Pose, Quad, Rect, ShaderEffect, Spring,
};
use rouge_render::DrawList;
use std::error::Error;

#[test]
fn translucent_layers_with_overlapping_content_fade_as_a_group() -> TestResult {
    let mut stage = StageBuilder::default().child(overlapping()).build()?;
    stage.set(stage.child, LayerProp::Opacity, 0.5);
    let stats = stage.draw();
    assert_eq!(
        (stats.groups, stats.group_passes),
        (1, 1),
        "rasterized once"
    );
    let img = stage.image()?;
    assert_near(
        img.pixel(10, 10),
        [255, 127, 127, 255],
        "red at half opacity",
    );
    assert_near(
        img.pixel(30, 10),
        [127, 127, 255, 255],
        "blue hides the red beneath it",
    );
    assert_eq!(img.pixel(70, 10), WHITE);

    assert_eq!(
        stage.draw().group_passes,
        0,
        "an unchanged group reuses its texture"
    );
    stage.set(stage.child, LayerProp::Opacity, 0.25);
    assert_eq!(
        stage.draw().group_passes,
        0,
        "opacity alone does not re-rasterize"
    );
    assert_near(
        stage.image()?.pixel(30, 10),
        [191, 191, 255, 255],
        "faded further",
    );

    stage.content(
        stage.child,
        vec![quads([Quad::new(
            Rect::new(0.0, 0.0, 40.0, 40.0),
            Color::RED,
        )])],
        vec![],
    );
    let stats = stage.draw();
    assert_eq!(stats.groups, 0, "a single primitive cannot overlap itself");
    Ok(())
}

#[test]
fn opaque_layers_are_replayed_directly_unless_asked_to_rasterize() -> TestResult {
    let mut stage = StageBuilder::default().child(overlapping()).build()?;
    assert_eq!(stage.draw().groups, 0);
    let direct = stage.image()?;

    stage.layers.set_rasterize(stage.child, true);
    stage.set(stage.child, LayerProp::TranslateX, 20.0);
    assert_eq!(stage.draw().groups, 1);
    let rasterized = stage.image()?;
    assert_eq!(direct.pixel(30, 10), BLUE);
    assert_eq!(
        rasterized.pixel(50, 10),
        BLUE,
        "the texture is drawn where the layer is"
    );
    assert_eq!(rasterized.pixel(10, 10), WHITE);
    assert_eq!(
        rasterized.pixel(79, 39),
        BLUE,
        "pixel exact at the far corner"
    );
    assert_eq!(rasterized.pixel(80, 40), WHITE);
    Ok(())
}

#[test]
fn nested_layers_are_clipped_by_their_parent() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(quads([Quad::new(
            Rect::new(0.0, 0.0, 60.0, 60.0),
            Color::RED,
        )]))
        .clip(Rect::new(10.0, 10.0, 20.0, 20.0))
        .build()?;
    stage.draw();
    let img = stage.image()?;
    assert_eq!(img.pixel(15, 15), RED);
    assert_eq!(img.pixel(5, 15), WHITE, "left of the clip");
    assert_eq!(img.pixel(35, 35), WHITE, "beyond the clip");

    stage.set(stage.root, LayerProp::TranslateX, 30.0);
    stage.draw();
    let img = stage.image()?;
    assert_eq!(
        img.pixel(45, 15),
        RED,
        "the clip moves with the layer that owns it"
    );
    assert_eq!(img.pixel(15, 15), [0, 0, 0, 0], "nothing left behind");
    Ok(())
}

#[test]
fn scroll_offset_moves_content_and_is_clamped() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(quads([
            Quad::new(Rect::new(0.0, 0.0, 100.0, 100.0), Color::RED),
            Quad::new(Rect::new(0.0, 100.0, 100.0, 100.0), Color::BLUE),
        ]))
        .build()?;
    stage.scroll_extent(200.0);
    stage.layers.scroll_by(stage.child, Point::new(0.0, 60.0));
    stage.draw();
    assert_eq!(stage.image()?.pixel(50, 30), RED);
    assert_eq!(stage.image()?.pixel(50, 50), BLUE);
    assert_eq!(stage.offset(), Point::new(0.0, 60.0));

    stage
        .layers
        .scroll_by(stage.child, Point::new(-500.0, 500.0));
    assert_eq!(stage.offset(), Point::new(0.0, 100.0));
    stage.scroll_extent(130.0);
    assert_eq!(stage.offset(), Point::new(0.0, 30.0));
    assert!(
        !stage.layers.is_animating(),
        "scrolling is not an animation"
    );
    Ok(())
}

#[test]
fn layout_transition_animates_from_the_old_frame() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage
        .layers
        .set_layout_transition(stage.child, Some(LINEAR.into()));
    stage
        .layers
        .set_frame(stage.child, Rect::new(60.0, 0.0, 20.0, 20.0));
    assert!(stage.layers.is_animating());
    stage.draw();
    assert_eq!(
        stage.image()?.pixel(10, 10),
        RED,
        "still drawn at the old position"
    );

    stage.run(0.5);
    stage.draw();
    let img = stage.image()?;
    assert_eq!(
        (img.pixel(10, 10), img.pixel(40, 10)),
        (WHITE, RED),
        "halfway there"
    );

    stage
        .layers
        .set_frame(stage.child, Rect::new(0.0, 0.0, 20.0, 20.0));
    stage.draw();
    assert_eq!(
        stage.image()?.pixel(40, 10),
        RED,
        "retargeting starts from the on-screen position"
    );
    stage.run(1.1);
    stage.draw();
    assert_eq!(stage.image()?.pixel(10, 10), RED);
    assert!(!stage.layers.is_animating());
    Ok(())
}

#[test]
fn one_shot_keyframes_hold_their_final_pose_and_looping_ones_repeat() -> TestResult {
    let slide = Keyframes::new()
        .at(0.0, |pose| pose.translate_x(0.0))
        .at(1.0, |pose| pose.translate_x(60.0))
        .duration(1.0);
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.layers.animate(stage.child, slide.clone());
    stage.run(0.5);
    assert_eq!(stage.x(stage.child), 30.0);
    stage.run(0.6);
    assert_eq!(stage.x(stage.child), 60.0, "fills forwards");
    assert!(!stage.layers.is_animating());

    stage.layers.animate(stage.child, slide.repeat());
    stage.run(1.4);
    assert!(
        (stage.x(stage.child) - 30.0).abs() < 0.01,
        "wrapped around, on the clock's phase rather than its start's: {}",
        stage.x(stage.child)
    );
    assert!(stage.layers.is_animating(), "a loop never settles");
    Ok(())
}

#[test]
fn loops_started_at_different_times_stay_in_phase() -> TestResult {
    let steps = Keyframes::new()
        .at(0.0, |pose| pose.translate_x(0.0))
        .at(0.5, |pose| pose.translate_x(0.0))
        .at(0.5, |pose| pose.translate_x(10.0))
        .at(1.0, |pose| pose.translate_x(10.0))
        .duration(0.2)
        .repeat();
    let mut stage = StageBuilder::default().child(square()).build()?;
    let (first, second) = (stage.root, stage.child);
    stage.layers.animate(first, steps.clone());
    stage.run(0.33);
    stage.layers.animate(second, steps);
    for frame in 0..40 {
        stage.run(0.01);
        assert_eq!(stage.x(first), stage.x(second), "frame {frame}");
    }
    Ok(())
}

#[test]
fn enter_starts_from_a_pose_and_exit_keeps_the_layer_until_it_arrives() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    let from = Pose::new().opacity(0.0).translate_x(40.0);
    stage.layers.enter(stage.child, from, LINEAR.into());
    assert_eq!(stage.x(stage.child), 40.0);
    stage.run(0.5);
    assert_eq!(stage.x(stage.child), 20.0);
    stage.run(0.6);
    assert_eq!(stage.x(stage.child), 0.0, "settles on the declared values");

    stage.exit(Pose::new().translate_x(60.0));
    stage.content(stage.root, vec![background()], vec![]);
    assert_eq!(
        stage.layers.layer_count(),
        2,
        "an exiting layer outlives its place in the tree"
    );
    stage.run(0.5);
    stage.draw();
    assert_eq!(
        stage.image()?.pixel(40, 10),
        RED,
        "still composited while it leaves"
    );
    stage.run(0.6);
    assert_eq!(stage.layers.layer_count(), 1, "then it is released");
    stage.draw();
    assert_eq!(stage.image()?.pixel(70, 10), WHITE);
    assert!(!stage.layers.is_animating());
    Ok(())
}

#[test]
fn removing_a_parent_releases_its_exiting_children() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.exit(Pose::new().opacity(0.0));
    stage.content(stage.root, vec![background()], vec![]);
    stage.layers.remove_layer(stage.root);
    assert_eq!(stage.layers.layer_count(), 0);
    Ok(())
}

#[test]
fn motions_drive_bound_props_and_report_completion() -> TestResult {
    let motion = MotionId(7);
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.bind(motion, 10.0);
    assert_eq!(stage.x(stage.child), 10.0);

    let mut sliding = stage.animate(motion, 50.0, LINEAR.into());
    assert_eq!(stage.layers.completion_deadline(), Some(1.0));
    stage.run(0.5);
    assert!(
        stage
            .layers
            .motion(motion)
            .is_some_and(|value| (value - 30.0).abs() < 0.01)
    );
    assert_eq!(stage.x(stage.child), 30.0);
    assert_eq!(settled(&mut sliding), None, "still running");
    stage.run(0.6);
    assert_eq!(settled(&mut sliding), Some(true));
    assert_eq!(stage.layers.completion_deadline(), None);
    assert!(!stage.layers.is_animating());

    let mut interrupted = stage.animate(motion, 0.0, Spring::smooth().into());
    stage.run(0.1);
    stage.layers.set_motion(motion, 5.0);
    assert_eq!(
        settled(&mut interrupted),
        Some(false),
        "a set interrupts the animation"
    );
    assert_eq!(stage.x(stage.child), 5.0);
    stage.run(0.1);
    assert!(!stage.layers.is_animating());

    let mut at_rest = stage.animate(motion, 5.0, LINEAR.into());
    assert_eq!(settled(&mut at_rest), Some(true), "already there");
    Ok(())
}

#[test]
fn layout_transitions_can_animate_size_by_stretching_the_layer() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage
        .layers
        .set_layout_transition(stage.child, Some(LayoutTransition::new(LINEAR).size()));
    stage
        .layers
        .set_frame(stage.child, Rect::new(0.0, 0.0, 40.0, 20.0));
    let edge = |stage: &mut Stage, inside: u32| {
        stage.draw();
        let img = stage.image()?;
        Ok::<_, Box<dyn Error>>((img.pixel(inside, 10), img.pixel(inside + 2, 10)))
    };
    assert_eq!(
        edge(&mut stage, 9)?,
        (RED, WHITE),
        "content starts at half its new width"
    );
    stage.run(0.5);
    assert_eq!(
        edge(&mut stage, 14)?,
        (RED, WHITE),
        "three quarters of the way"
    );
    stage.run(0.6);
    assert_eq!(
        edge(&mut stage, 19)?,
        (RED, WHITE),
        "and settles unstretched"
    );
    assert!(!stage.layers.is_animating());
    Ok(())
}

#[test]
fn fill_and_corner_radius_animate_without_uploading_instances() -> TestResult {
    let mut backing = DrawList::new();
    backing.push_quad(Quad::backing(Rect::new(0.0, 0.0, 40.0, 40.0)));
    let mut stage = StageBuilder::default().child(backing).build()?;
    stage
        .layers
        .set_frame(stage.child, Rect::new(0.0, 0.0, 40.0, 40.0));
    stage.pose(stage.child, Pose::new().background(Color::RED), false);
    stage.draw();
    assert_eq!(
        (stage.image()?.pixel(20, 20), stage.image()?.pixel(1, 1)),
        (RED, RED)
    );

    stage
        .layers
        .set_transition(stage.child, Some(LINEAR.into()));
    stage.pose(
        stage.child,
        Pose::new()
            .background(Color::BLUE)
            .rounded(Corners::all(20.0)),
        true,
    );
    stage.run(0.5);
    let stats = stage.draw();
    assert_eq!(
        stats.instance_bytes_uploaded, 0,
        "the draw list is untouched"
    );
    assert_near(
        stage.image()?.pixel(20, 20),
        [128, 0, 128, 255],
        "halfway between red and blue",
    );
    assert_eq!(
        stage.image()?.pixel(1, 1),
        WHITE,
        "the corner is already cut"
    );
    stage.run(0.6);
    stage.draw();
    assert_eq!(stage.image()?.pixel(20, 20), BLUE);
    assert_eq!(stage.image()?.pixel(5, 5), WHITE, "a full circle");
    assert!(!stage.layers.is_animating());
    Ok(())
}

#[test]
fn decay_coasts_to_rest_from_a_velocity() -> TestResult {
    let motion = MotionId(1);
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.bind(motion, 0.0);
    let (done, mut coasting) = Animation::pending();
    let decay = Decay::new().rate(2.0).rest(1.0);
    assert_eq!(decay.distance(100.0), 50.0);
    stage.layers.decay_motion(motion, 100.0, decay, done);
    stage.run(0.1);
    let early = stage.x(stage.child);
    assert!(
        early > 8.0 && early < 10.0,
        "starts at the given velocity: {early}"
    );
    assert_eq!(settled(&mut coasting), None);
    stage.run(5.0);
    assert!(
        (stage.x(stage.child) - 49.5).abs() < 0.1,
        "stops just short of its projection: {}",
        stage.x(stage.child)
    );
    assert_eq!(settled(&mut coasting), Some(true));
    assert!(!stage.layers.is_animating());
    Ok(())
}

#[test]
fn dragging_past_the_limit_rubber_bands_and_release_springs_back() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    let offset = |stage: &Stage| stage.offset().y;
    stage.scroll_extent(200.0);
    stage
        .layers
        .scroll_drag(stage.child, Point::new(-30.0, -40.0));
    assert_eq!(stage.offset().x, 0.0, "nothing to scroll sideways");
    assert_eq!(offset(&stage), -40.0);
    stage
        .layers
        .scroll_drag(stage.child, Point::new(0.0, -40.0));
    let pulled = offset(&stage);
    assert!(
        pulled < -40.0 && pulled > -50.0,
        "further pulling meets resistance: {pulled}"
    );
    assert!(!stage.layers.is_animating(), "held in place while dragging");

    stage.layers.scroll_release(stage.child, Point::ZERO);
    assert!(stage.layers.is_animating());
    stage.run(0.1);
    assert!(
        offset(&stage) > pulled && offset(&stage) < 0.0,
        "on its way back"
    );
    stage.run(2.0);
    assert!(offset(&stage).abs() < 0.01);
    assert!(!stage.layers.is_animating());

    stage
        .layers
        .scroll_release(stage.child, Point::new(0.0, 100.0));
    stage.run(0.1);
    let coasting = offset(&stage);
    assert!(
        coasting > 8.0 && coasting < 10.0,
        "momentum carries on after release: {coasting}"
    );
    stage.run(5.0);
    assert!((offset(&stage) - 49.5).abs() < 0.1, "and decays to rest");

    stage
        .layers
        .scroll_release(stage.child, Point::new(0.0, 3000.0));
    let mut peak = 0.0f32;
    for _ in 0..300 {
        stage.run(0.01);
        peak = peak.max(offset(&stage));
    }
    assert!(peak > 101.0, "momentum overshoots the end: {peak}");
    assert!((offset(&stage) - 100.0).abs() < 0.01, "then settles on it");
    assert!(!stage.layers.is_animating());
    Ok(())
}

#[test]
fn content_blur_spreads_past_the_layer_and_is_cached() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(square())
        .at(40.0, 40.0)
        .build()?;
    stage.effect(stage.child, EffectSlot::Content(0), &Blur::new(4.0), false);
    let stats = stage.draw();
    assert_eq!(
        (stats.groups, stats.group_passes, stats.content_blurs),
        (1, 1, 1)
    );
    let img = stage.image()?;
    let green = |x, y| img.pixel(x, y)[1];
    assert!(
        green(50, 50) < 40,
        "mostly red in the middle: {:?}",
        img.pixel(50, 50)
    );
    assert!(
        (60..200).contains(&green(40, 50)),
        "a soft edge: {:?}",
        img.pixel(40, 50)
    );
    assert!(
        (200..255).contains(&green(36, 50)),
        "bleeding outside the layer: {:?}",
        img.pixel(36, 50)
    );
    assert_eq!(img.pixel(20, 50), WHITE, "but not far");

    let stats = stage.draw();
    assert_eq!(
        (stats.group_passes, stats.content_blurs),
        (0, 0),
        "nothing changed, nothing re-rendered"
    );
    Ok(())
}

#[test]
fn effect_parameters_animate_with_the_layer_transition() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(square())
        .at(40.0, 40.0)
        .build()?;
    stage.effect(
        stage.child,
        EffectSlot::Content(0),
        &ColorMatrix::saturate(1.0),
        false,
    );
    stage
        .layers
        .set_transition(stage.child, Some(LINEAR.into()));
    stage.effect(
        stage.child,
        EffectSlot::Content(0),
        &ColorMatrix::grayscale(),
        true,
    );
    assert!(stage.layers.is_animating());
    stage.draw();
    assert_eq!(
        stage.image()?.pixel(50, 50),
        RED,
        "starts from the old parameters"
    );
    stage.run(0.5);
    let stats = stage.draw();
    assert_eq!(
        stats.group_passes, 0,
        "only the effect is re-run, the content is not re-rendered"
    );
    assert_near(
        stage.image()?.pixel(50, 50),
        [155, 27, 27, 255],
        "half desaturated",
    );
    stage.run(0.6);
    stage.draw();
    assert_near(
        stage.image()?.pixel(50, 50),
        [54, 54, 54, 255],
        "red at its luma",
    );
    assert!(!stage.layers.is_animating());
    Ok(())
}

#[test]
fn layer_origins_snap_to_device_pixels() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage
        .layers
        .set_frame(stage.child, Rect::new(10.4, 0.0, 20.0, 20.0));
    stage.draw();
    let img = stage.image()?;
    assert_eq!(
        (img.pixel(9, 10), img.pixel(10, 10)),
        (WHITE, RED),
        "crisp left edge"
    );
    assert_eq!(
        (img.pixel(29, 10), img.pixel(30, 10)),
        (RED, WHITE),
        "crisp right edge"
    );
    Ok(())
}

#[test]
fn a_clocked_effect_runs_on_compositor_time_until_it_is_removed() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(square())
        .at(40.0, 40.0)
        .build()?;
    stage.effect(stage.child, EffectSlot::Content(0), &Sweep, false);
    assert!(stage.layers.is_animating(), "a clock never settles");
    stage.draw();
    assert_eq!(stage.image()?.pixel(42, 50), RED, "nothing swept yet");

    stage.run(0.5);
    stage.effect(stage.child, EffectSlot::Content(0), &Sweep, false);
    stage.draw();
    let img = stage.image()?;
    assert_eq!(
        (img.pixel(42, 50), img.pixel(58, 50)),
        (BLUE, RED),
        "half swept, and applying it again keeps its time"
    );

    stage.effect_off(stage.child, EffectSlot::Content(0));
    stage.run(0.1);
    assert!(!stage.layers.is_animating());
    Ok(())
}

#[test]
fn effects_chain_in_order_before_the_mask() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(square())
        .at(40.0, 40.0)
        .build()?;
    stage.effect(
        stage.child,
        EffectSlot::Content(0),
        &ColorMatrix::grayscale(),
        false,
    );
    stage.effect(
        stage.child,
        EffectSlot::Content(1),
        &ColorMatrix::invert(),
        false,
    );
    let fade = Mask::linear_gradient(Point::new(0.0, 0.0), Point::new(1.0, 0.0));
    stage.effect(stage.child, EffectSlot::Mask, &fade, false);
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

    stage.effect_off(stage.child, EffectSlot::Content(1));
    stage.effect_off(stage.child, EffectSlot::Mask);
    stage.draw();
    assert_near(
        stage.image()?.pixel(59, 50),
        [54, 54, 54, 255],
        "removing a stage restores the rest",
    );
    Ok(())
}

#[test]
fn a_fresh_outer_stage_keeps_its_inner_stage_cached() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(square())
        .at(40.0, 40.0)
        .build()?;
    stage.effect(
        stage.child,
        EffectSlot::Content(0),
        &ColorMatrix::saturate(1.0),
        false,
    );
    stage.effect(
        stage.child,
        EffectSlot::Content(1),
        &ColorMatrix::invert(),
        false,
    );
    assert_eq!(stage.draw().group_passes, 2);
    for _ in 0..3 {
        assert_eq!(stage.draw().group_passes, 0, "nothing changed");
    }
    stage.effect(
        stage.child,
        EffectSlot::Content(0),
        &ColorMatrix::saturate(0.5),
        false,
    );
    assert_eq!(
        stage.draw().group_passes,
        1,
        "only the outer stage reads the changed effect"
    );
    Ok(())
}

#[test]
fn a_translucent_group_slides_and_fades_without_rasterizing_again() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(overlapping())
        .at(10.0, 10.0)
        .sized(60.0, 40.0)
        .build()?;
    stage.set(stage.child, LayerProp::Opacity, 0.5);
    assert_eq!(stage.draw().group_passes, 1);
    stage
        .layers
        .set_transition(stage.child, Some(LINEAR.into()));
    stage
        .layers
        .set_prop(stage.child, LayerProp::TranslateX, 20.0, true);
    stage
        .layers
        .set_prop(stage.child, LayerProp::Opacity, 0.8, true);
    for _ in 0..11 {
        stage.run(0.1);
        assert_eq!(stage.draw().group_passes, 0, "moved, not re-rendered");
    }
    assert!(!stage.layers.is_animating());
    let img = stage.image()?;
    assert_eq!(img.pixel(20, 20), WHITE, "the red half left its old place");
    assert_near(img.pixel(35, 20), [255, 51, 51, 255], "red at 0.8");
    assert_near(img.pixel(70, 20), [51, 51, 255, 255], "blue on top");
    Ok(())
}

#[test]
fn a_group_faded_out_and_back_reuses_its_texture() -> TestResult {
    let mut stage = StageBuilder::default().child(overlapping()).build()?;
    stage.set(stage.child, LayerProp::Opacity, 0.5);
    assert_eq!(stage.draw().group_passes, 1);
    stage.set(stage.child, LayerProp::Opacity, 0.0);
    assert_eq!(stage.draw().groups, 0, "hidden");
    stage.set(stage.child, LayerProp::Opacity, 0.5);
    let stats = stage.draw();
    assert_eq!(
        (stats.groups, stats.group_passes),
        (1, 0),
        "shown again from the cache"
    );
    Ok(())
}

#[test]
fn masks_can_clip_to_the_rounded_shape_or_fade_radially() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(square())
        .at(40.0, 40.0)
        .build()?;
    stage.pose(stage.child, Pose::new().rounded(Corners::all(10.0)), false);
    stage.effect(stage.child, EffectSlot::Mask, &Mask::shape(), false);
    stage.draw();
    assert_eq!(
        (stage.image()?.pixel(50, 50), stage.image()?.pixel(41, 41)),
        (RED, WHITE)
    );

    let spot = Mask::radial_gradient(Point::new(0.5, 0.5), 0.5)
        .stops(0.5, 1.0)
        .invert();
    stage.effect(stage.child, EffectSlot::Mask, &spot, false);
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
        stage.effect(stage.child, EffectSlot::Blend, &mode, false);
        let stats = stage.draw();
        let img = stage.image()?;
        Ok::<_, Box<dyn Error>>(((img.pixel(45, 50), img.pixel(55, 50)), stats))
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
        stage.child,
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

    stage.set(stage.child, LayerProp::TranslateX, 5.0);
    stage.draw();
    assert_eq!(
        stage.image()?.pixel(47, 50),
        RED,
        "local coordinates follow the layer"
    );
    Ok(())
}

fn overlapping() -> DrawList {
    quads([
        Quad::new(Rect::new(0.0, 0.0, 40.0, 40.0), Color::RED),
        Quad::new(Rect::new(20.0, 0.0, 40.0, 40.0), Color::BLUE),
    ])
}

struct Sweep;

impl ShaderEffect for Sweep {
    const WGSL: &'static str = "
        fn effect(in: EffectInput) -> vec4<f32> {
            let swept = step(in.uv.x, param(0).x);
            return mix(sample_content(in.local), vec4<f32>(0.0, 0.0, 1.0, 1.0), swept);
        }
    ";
    const INPUTS: Inputs = Inputs::CONTENT;
    const CLOCK: Option<usize> = Some(0);
}

struct Stripes {
    period: f32,
    tint: Color,
}

impl ShaderEffect for Stripes {
    const WGSL: &'static str = "
        fn effect(in: EffectInput) -> vec4<f32> {
            let content = sample_content(in.local);
            let odd = floor(in.local.x * 2.0 / param(0).x) % 2.0;
            return mix(content, param(1) * content.a, odd);
        }
    ";
    const INPUTS: Inputs = Inputs::CONTENT;

    fn uniforms(&self, uniforms: &mut [f32; 16]) {
        uniforms[0] = self.period;
        uniforms[4..8].copy_from_slice(&self.tint.to_array());
    }
}
