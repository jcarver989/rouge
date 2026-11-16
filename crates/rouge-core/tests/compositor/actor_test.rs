#[path = "../common/mod.rs"]
mod common;

use common::{Stage, StageBuilder, WINDOW, square};
use rouge_core::testing::{LINEAR, TestResult};
use rouge_core::{CompositorEvent, LayerProp, Message};
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
    assert_eq!(
        stage.actor.next_wake(1.5),
        None,
        "and the compositor idles at the end"
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

fn sliding() -> Result<Stage, Box<dyn Error>> {
    let mut stage = StageBuilder::default().child(square()).build()?;
    (stage.layers).set_transition(stage.child, Some(LINEAR.into()));
    (stage.layers).set_prop(stage.child, LayerProp::TranslateX, 40.0, true);
    stage.sync();
    Ok(stage)
}
