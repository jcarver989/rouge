#[path = "../common/mod.rs"]
mod common;

use common::{Stage, StageBuilder, WINDOW, square};
use rouge_core::testing::{LINEAR, TestResult};
use rouge_core::{CompositorEvent, Keyframes, LayerProp, Message, MotionId};
use rouge_render::headless::assert_near;
use std::error::Error;

#[test]
fn a_commit_is_acknowledged_once_it_is_presented() -> TestResult {
    let mut stage = StageBuilder::default().build()?;
    assert_eq!(
        stage.presented(),
        0,
        "nothing is acknowledged before a present"
    );
    stage.frame(0.0);
    assert_eq!(stage.presented(), 1);
    assert_eq!(stage.actor.next_wake(0.0), None, "idle once acknowledged");
    Ok(())
}

#[test]
fn a_compositor_only_animation_sends_nothing_back() -> TestResult {
    let mut stage = sliding()?;
    stage.frame(0.0);
    assert_eq!(stage.presented(), 1);
    for step in 1..=9 {
        let now = f64::from(step) * 0.1;
        assert!(
            stage.actor.next_wake(now).is_some(),
            "still animating at {now}"
        );
        stage.frame(now);
    }
    assert_eq!(
        stage.presented(),
        0,
        "no message while the UI is not watching"
    );
    assert!(
        stage.actor.next_wake(1.5).is_some(),
        "the settled pose is still to be shown"
    );
    stage.frame(1.5);
    assert_eq!(
        stage.actor.next_wake(1.6),
        None,
        "and then the compositor idles"
    );
    Ok(())
}

#[test]
fn a_layer_that_animates_again_after_settling_wakes_the_compositor_again() -> TestResult {
    let mut stage = sliding()?;
    stage.frame(0.0);
    stage.frame(1.5);
    assert_eq!(stage.actor.next_wake(1.6), None);
    stage.run(2.0);
    (stage.layers).set_prop(stage.child, LayerProp::TranslateX, 80.0, true);
    stage.sync();
    stage.frame(2.0);
    assert!(
        stage.actor.next_wake(2.1).is_some(),
        "the second slide is still running"
    );
    Ok(())
}

#[test]
fn a_motion_that_animates_again_after_settling_wakes_the_compositor_again() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.bind(MotionId(1), 0.0);
    let _first = stage.animate(MotionId(1), 40.0, LINEAR.into());
    stage.sync();
    stage.frame(0.0);
    stage.frame(1.5);
    assert_eq!(stage.actor.next_wake(1.6), None);
    stage.run(2.0);
    let _second = stage.animate(MotionId(1), 80.0, LINEAR.into());
    stage.sync();
    stage.frame(2.0);
    assert!(
        stage.actor.next_wake(2.1).is_some(),
        "the second motion is still running"
    );
    Ok(())
}

#[test]
fn a_held_keyframe_wakes_the_compositor_only_when_its_pose_changes() -> TestResult {
    let mut stage = stepping(
        Keyframes::new()
            .at(0.0, |pose| pose.translate_x(0.0))
            .at(0.5, |pose| pose.translate_x(0.0))
            .at(0.5, |pose| pose.translate_x(10.0))
            .at(1.0, |pose| pose.translate_x(10.0)),
    )?;
    stage.frame(0.0);
    stage.presented();
    assert_wakes_at(&mut stage, 0.1, 0.5);
    stage.frame(0.5);
    assert_wakes_at(&mut stage, 0.6, 1.0);
    Ok(())
}

#[test]
fn a_held_keyframe_starting_with_a_jump_wakes_at_each_repeat() -> TestResult {
    let mut stage = stepping(
        Keyframes::new()
            .at(0.0, |pose| pose.translate_x(20.0))
            .at(0.0, |pose| pose.translate_x(0.0))
            .at(0.5, |pose| pose.translate_x(0.0))
            .at(0.5, |pose| pose.translate_x(10.0))
            .at(1.0, |pose| pose.translate_x(10.0)),
    )?;
    stage.frame(0.0);
    stage.presented();
    assert_wakes_at(&mut stage, 0.1, 0.5);
    stage.frame(0.5);
    assert_wakes_at(&mut stage, 0.6, 1.0);
    stage.frame(1.0);
    assert_wakes_at(&mut stage, 1.1, 1.5);
    Ok(())
}

#[test]
fn a_frame_presented_during_a_step_is_followed_by_one_showing_where_it_settled() -> TestResult {
    let mut stage = stepping(
        Keyframes::new()
            .at(0.0, |pose| pose.translate_x(0.0))
            .at(0.49, |pose| pose.translate_x(0.0))
            .at(0.5, |pose| pose.translate_x(10.0))
            .at(1.0, |pose| pose.translate_x(10.0)),
    )?;
    stage.frame(0.0);
    stage.presented();
    stage.frame(0.495);
    let wake = stage.actor.next_wake(0.6);
    assert!(
        wake.is_some_and(|wake| wake <= 0.6),
        "the settled step is still to be shown, not at {wake:?}"
    );
    stage.frame(0.6);
    assert_wakes_at(&mut stage, 0.7, 1.0);
    Ok(())
}

#[test]
fn an_interpolated_keyframe_wakes_the_compositor_every_frame() -> TestResult {
    let mut stage = stepping(
        Keyframes::new()
            .at(0.0, |pose| pose.translate_x(0.0))
            .at(1.0, |pose| pose.translate_x(10.0)),
    )?;
    stage.frame(0.0);
    stage.presented();
    let wake = stage.actor.next_wake(0.1);
    assert!(
        wake.is_some_and(|wake| wake <= 0.1),
        "a frame is due at once, not {wake:?}"
    );
    Ok(())
}

#[test]
fn an_empty_commit_asks_for_a_frame_and_is_answered_after_the_next_present() -> TestResult {
    let mut stage = sliding()?;
    stage.frame(0.0);
    stage.presented();
    stage.sync();
    assert_eq!(stage.presented(), 0);
    stage.frame(0.05);
    assert_eq!(stage.presented(), 1);
    Ok(())
}

#[test]
fn a_hidden_window_acknowledges_changes_at_once_and_holds_frame_requests() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.frame(0.0);
    stage.presented();
    stage.show(false);
    stage.set(stage.child, LayerProp::TranslateX, 5.0);
    stage.sync();
    assert_eq!(
        stage.presented(),
        1,
        "nothing will be presented, so it is acknowledged now"
    );
    stage.sync();
    stage.frame(0.1);
    assert_eq!(
        stage.presented(),
        0,
        "the frame request waits for the window"
    );
    stage.show(true);
    stage.frame(0.2);
    assert_eq!(stage.presented(), 1);
    Ok(())
}

#[test]
fn closing_a_window_reports_it() -> TestResult {
    let mut stage = StageBuilder::default().build()?;
    stage.actor.handle(Message::Close(WINDOW));
    let closed = stage.actor.events().last();
    assert_eq!(closed, Some((WINDOW, CompositorEvent::Closed)));
    Ok(())
}

#[test]
fn a_detached_window_hands_its_target_back_and_acknowledges_changes_at_once() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.frame(0.0);
    stage.presented();
    assert!(stage.detach().is_some(), "the target comes back");
    assert!(stage.detach().is_none(), "and only once");
    stage.set(stage.child, LayerProp::TranslateX, 5.0);
    stage.sync();
    assert_eq!(
        stage.presented(),
        1,
        "nothing will be presented, so it is acknowledged now"
    );
    assert_eq!(stage.actor.next_wake(0.1), None, "nothing to draw into");
    Ok(())
}

#[test]
fn an_attached_target_shows_what_was_committed_while_detached() -> TestResult {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage.frame(0.0);
    let target = stage.detach().ok_or("the stage has no target")?;
    stage.set(stage.child, LayerProp::TranslateX, 50.0);
    stage.sync();
    stage.attach(target);
    assert!(
        stage.actor.next_wake(0.1).is_some(),
        "the new target is still to be drawn"
    );
    stage.frame(0.1);
    assert_near(
        stage.image()?.pixel(60, 10),
        [255, 0, 0, 255],
        "moved square",
    );
    assert_eq!(stage.actor.next_wake(0.2), None, "and then it idles");
    Ok(())
}

fn stepping(keyframes: Keyframes) -> Result<Stage, Box<dyn Error>> {
    let mut stage = StageBuilder::default().child(square()).build()?;
    stage
        .layers
        .animate(stage.child, keyframes.duration(1.0).repeat());
    stage.sync();
    Ok(stage)
}

fn assert_wakes_at(stage: &mut Stage, now: f64, expected: f64) {
    let wake = stage.actor.next_wake(now);
    assert!(
        wake.is_some_and(|wake| (wake - expected).abs() < 1e-4),
        "at {now} the compositor wakes at {wake:?}, not {expected}"
    );
}

fn sliding() -> Result<Stage, Box<dyn Error>> {
    let mut stage = StageBuilder::default().child(square()).build()?;
    (stage.layers).set_transition(stage.child, Some(LINEAR.into()));
    (stage.layers).set_prop(stage.child, LayerProp::TranslateX, 40.0, true);
    stage.sync();
    Ok(stage)
}
