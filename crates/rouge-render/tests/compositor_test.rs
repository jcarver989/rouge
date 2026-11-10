mod common;

use common::{
    BLUE, CHILD, LINEAR, RED, ROOT, Stage, StageBuilder, TestResult, WHITE, background, quads,
    square,
};
use rouge_render::headless::assert_near;
use rouge_render::{
    Blur, Color, ColorMatrix, Command, Decay, DrawList, EffectSlot, GpuError, Keyframes,
    LayerCommand, LayerProp, LayoutTransition, MotionCommand, MotionId, Point, Pose, Quad, Rect,
    Spring,
};
use tokio::sync::oneshot::channel;

#[test]
fn translucent_layers_with_overlapping_content_fade_as_a_group() -> TestResult {
    let mut stage = StageBuilder::default().child(overlapping()).build()?;
    stage.set(CHILD, LayerProp::Opacity, 0.5);
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
    stage.set(CHILD, LayerProp::Opacity, 0.25);
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
        CHILD,
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

    stage.layer(CHILD, LayerCommand::Rasterize(true));
    stage.set(CHILD, LayerProp::TranslateX, 20.0);
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

    stage.set(ROOT, LayerProp::TranslateX, 30.0);
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
    stage.layer(CHILD, LayerCommand::ScrollBy(Point::new(0.0, 60.0)));
    stage.draw();
    assert_eq!(stage.image()?.pixel(50, 30), RED);
    assert_eq!(stage.image()?.pixel(50, 50), BLUE);
    assert_eq!(stage.offset(), Point::new(0.0, 60.0));

    stage.layer(CHILD, LayerCommand::ScrollBy(Point::new(-500.0, 500.0)));
    assert_eq!(stage.offset(), Point::new(0.0, 100.0));
    stage.scroll_extent(130.0);
    assert_eq!(stage.offset(), Point::new(0.0, 30.0));
    assert!(
        !stage.compositor.is_animating(),
        "scrolling is not an animation"
    );
    Ok(())
}

#[test]
fn layout_transition_animates_from_the_old_frame() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.layer(CHILD, LayerCommand::LayoutTransition(Some(LINEAR.into())));
    stage.layer(CHILD, LayerCommand::Frame(Rect::new(60.0, 0.0, 20.0, 20.0)));
    assert!(stage.compositor.is_animating());
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

    stage.layer(CHILD, LayerCommand::Frame(Rect::new(0.0, 0.0, 20.0, 20.0)));
    stage.draw();
    assert_eq!(
        stage.image()?.pixel(40, 10),
        RED,
        "retargeting starts from the on-screen position"
    );
    stage.run(1.1);
    stage.draw();
    assert_eq!(stage.image()?.pixel(10, 10), RED);
    assert!(!stage.compositor.is_animating());
    Ok(())
}

#[test]
fn one_shot_keyframes_hold_their_final_pose_and_looping_ones_repeat() -> TestResult {
    let slide = Keyframes::new()
        .at(0.0, |pose| pose.translate_x(0.0))
        .at(1.0, |pose| pose.translate_x(60.0))
        .duration(1.0);
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.layer(CHILD, LayerCommand::Animate(slide.clone()));
    stage.run(0.5);
    assert_eq!(stage.x(CHILD), 30.0);
    stage.run(0.6);
    assert_eq!(stage.x(CHILD), 60.0, "fills forwards");
    assert!(!stage.compositor.is_animating());

    stage.layer(CHILD, LayerCommand::Animate(slide.repeat()));
    stage.run(1.5);
    assert_eq!(stage.x(CHILD), 30.0, "wrapped around");
    assert!(stage.compositor.is_animating(), "a loop never settles");
    Ok(())
}

#[test]
fn enter_starts_from_a_pose_and_exit_keeps_the_layer_until_it_arrives() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    let from = Pose::new().opacity(0.0).translate_x(40.0);
    stage.layer(
        CHILD,
        LayerCommand::Enter {
            from,
            timing: LINEAR.into(),
        },
    );
    assert_eq!(stage.x(CHILD), 40.0);
    stage.run(0.5);
    assert_eq!(stage.x(CHILD), 20.0);
    stage.run(0.6);
    assert_eq!(stage.x(CHILD), 0.0, "settles on the declared values");

    stage.exit(Pose::new().translate_x(60.0));
    stage.content(ROOT, vec![background()], vec![]);
    assert_eq!(
        stage.compositor.layer_count(),
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
    assert_eq!(stage.compositor.layer_count(), 1, "then it is released");
    stage.draw();
    assert_eq!(stage.image()?.pixel(70, 10), WHITE);
    assert!(!stage.compositor.is_animating());
    Ok(())
}

#[test]
fn removing_a_parent_releases_its_exiting_children() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.exit(Pose::new().opacity(0.0));
    stage.content(ROOT, vec![background()], vec![]);
    stage.apply(Command::Remove(ROOT));
    assert_eq!(stage.compositor.layer_count(), 0);
    Ok(())
}

#[test]
fn motions_drive_bound_props_and_report_completion() -> TestResult {
    let motion = MotionId(7);
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.bind(motion, 10.0);
    assert_eq!(stage.x(CHILD), 10.0);

    let mut settled = stage.animate(motion, 50.0, LINEAR.into());
    stage.run(0.5);
    assert!(
        stage
            .compositor
            .motion(motion)
            .is_some_and(|value| (value - 30.0).abs() < 0.01)
    );
    assert_eq!(stage.x(CHILD), 30.0);
    assert!(settled.try_recv().is_err(), "still running");
    stage.run(0.6);
    assert_eq!(settled.try_recv(), Ok(true));
    assert!(!stage.compositor.is_animating());

    let mut interrupted = stage.animate(motion, 0.0, Spring::smooth().into());
    stage.run(0.1);
    stage.apply(Command::Motion(motion, MotionCommand::Set(5.0)));
    assert_eq!(
        interrupted.try_recv(),
        Ok(false),
        "a set interrupts the animation"
    );
    assert_eq!(stage.x(CHILD), 5.0);
    stage.run(0.1);
    assert!(!stage.compositor.is_animating());

    let mut at_rest = stage.animate(motion, 5.0, LINEAR.into());
    assert_eq!(at_rest.try_recv(), Ok(true), "already there");
    Ok(())
}

#[test]
fn layout_transitions_can_animate_size_by_stretching_the_layer() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.layer(
        CHILD,
        LayerCommand::LayoutTransition(Some(LayoutTransition::new(LINEAR).size())),
    );
    stage.layer(CHILD, LayerCommand::Frame(Rect::new(0.0, 0.0, 40.0, 20.0)));
    let edge = |stage: &mut Stage, inside: u32| {
        stage.draw();
        let img = stage.image()?;
        Ok::<_, GpuError>((img.pixel(inside, 10), img.pixel(inside + 2, 10)))
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
    assert!(!stage.compositor.is_animating());
    Ok(())
}

#[test]
fn fill_and_corner_radius_animate_without_uploading_instances() -> TestResult {
    let mut backing = DrawList::new();
    backing.push_quad(Quad::backing(Rect::new(0.0, 0.0, 40.0, 40.0)));
    let mut stage = StageBuilder::default().child(backing).build()?;
    stage.layer(CHILD, LayerCommand::Frame(Rect::new(0.0, 0.0, 40.0, 40.0)));
    stage.pose(CHILD, Pose::new().background(Color::RED), false);
    stage.draw();
    assert_eq!(
        (stage.image()?.pixel(20, 20), stage.image()?.pixel(1, 1)),
        (RED, RED)
    );

    stage.layer(CHILD, LayerCommand::Transition(Some(LINEAR.into())));
    stage.pose(
        CHILD,
        Pose::new().background(Color::BLUE).rounded(20.0),
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
    assert!(!stage.compositor.is_animating());
    Ok(())
}

#[test]
fn decay_coasts_to_rest_from_a_velocity() -> TestResult {
    let motion = MotionId(1);
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.bind(motion, 0.0);
    let (done, mut settled) = channel();
    let decay = Decay::new().rate(2.0).rest(1.0);
    assert_eq!(decay.distance(100.0), 50.0);
    stage.apply(Command::Motion(
        motion,
        MotionCommand::Decay {
            velocity: 100.0,
            decay,
            done: Some(done),
        },
    ));
    stage.run(0.1);
    let early = stage.x(CHILD);
    assert!(
        early > 8.0 && early < 10.0,
        "starts at the given velocity: {early}"
    );
    assert!(settled.try_recv().is_err());
    stage.run(5.0);
    assert!(
        (stage.x(CHILD) - 49.5).abs() < 0.1,
        "stops just short of its projection: {}",
        stage.x(CHILD)
    );
    assert_eq!(settled.try_recv(), Ok(true));
    assert!(!stage.compositor.is_animating());
    Ok(())
}

#[test]
fn dragging_past_the_limit_rubber_bands_and_release_springs_back() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    let offset = |stage: &Stage| stage.offset().y;
    stage.scroll_extent(200.0);
    stage.layer(CHILD, LayerCommand::ScrollDrag(Point::new(-30.0, -40.0)));
    assert_eq!(stage.offset().x, 0.0, "nothing to scroll sideways");
    assert_eq!(offset(&stage), -40.0);
    stage.layer(CHILD, LayerCommand::ScrollDrag(Point::new(0.0, -40.0)));
    let pulled = offset(&stage);
    assert!(
        pulled < -40.0 && pulled > -50.0,
        "further pulling meets resistance: {pulled}"
    );
    assert!(
        !stage.compositor.is_animating(),
        "held in place while dragging"
    );

    stage.layer(CHILD, LayerCommand::ScrollRelease(Point::ZERO));
    assert!(stage.compositor.is_animating());
    stage.run(0.1);
    assert!(
        offset(&stage) > pulled && offset(&stage) < 0.0,
        "on its way back"
    );
    stage.run(2.0);
    assert!(offset(&stage).abs() < 0.01);
    assert!(!stage.compositor.is_animating());

    stage.layer(CHILD, LayerCommand::ScrollRelease(Point::new(0.0, 100.0)));
    stage.run(0.1);
    let coasting = offset(&stage);
    assert!(
        coasting > 8.0 && coasting < 10.0,
        "momentum carries on after release: {coasting}"
    );
    stage.run(5.0);
    assert!((offset(&stage) - 49.5).abs() < 0.1, "and decays to rest");

    stage.layer(CHILD, LayerCommand::ScrollRelease(Point::new(0.0, 3000.0)));
    let mut peak = 0.0f32;
    for _ in 0..300 {
        stage.run(0.01);
        peak = peak.max(offset(&stage));
    }
    assert!(peak > 101.0, "momentum overshoots the end: {peak}");
    assert!((offset(&stage) - 100.0).abs() < 0.01, "then settles on it");
    assert!(!stage.compositor.is_animating());
    Ok(())
}

#[test]
fn content_blur_spreads_past_the_layer_and_is_cached() -> TestResult {
    let mut stage = StageBuilder::default()
        .child(square())
        .at(40.0, 40.0)
        .build()?;
    stage.effect(CHILD, EffectSlot::Content(0), &Blur::new(4.0), false);
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
        CHILD,
        EffectSlot::Content(0),
        &ColorMatrix::saturate(1.0),
        false,
    );
    stage.layer(CHILD, LayerCommand::Transition(Some(LINEAR.into())));
    stage.effect(
        CHILD,
        EffectSlot::Content(0),
        &ColorMatrix::grayscale(),
        true,
    );
    assert!(stage.compositor.is_animating());
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
    assert!(!stage.compositor.is_animating());
    Ok(())
}

#[test]
fn layer_origins_snap_to_device_pixels() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.layer(CHILD, LayerCommand::Frame(Rect::new(10.4, 0.0, 20.0, 20.0)));
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

fn overlapping() -> DrawList {
    quads([
        Quad::new(Rect::new(0.0, 0.0, 40.0, 40.0), Color::RED),
        Quad::new(Rect::new(20.0, 0.0, 40.0, 40.0), Color::BLUE),
    ])
}
