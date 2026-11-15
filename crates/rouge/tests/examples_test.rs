#![cfg(not(target_arch = "wasm32"))]

#[allow(dead_code)]
#[path = "../examples/shared/mod.rs"]
mod shared;

use rouge::core::testing::{TestResult, TestUi, TestUiBuilder};
use rouge::prelude::*;
use rouge::widgets::bundled_fonts;
use shared::counter_view::counter;
use shared::diff::{Patch, SAMPLE, diff_view};
use shared::effects_view::effects_demo;
use shared::glass_view::glass;
use shared::motion_view::motion_demo;
use shared::settings_view::settings;

#[test]
fn the_counter_example_counts() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(560.0, 360.0, counter);
    ui.click_on("increment")?;
    ui.click_on("increment")?;
    assert!(ui.text_bounds("2").is_some());
    Ok(())
}

#[test]
fn the_glass_example_lifts_its_card() -> TestResult {
    let _entered = Runtime::new().enter();
    let lifted = signal(false);
    let mut ui = example(600.0, 380.0, move || glass(lifted));
    ui.click_at(300.0, 190.0);
    ui.run(0.5);
    assert!(lifted.get_untracked());
    ui.render()?;
    Ok(())
}

#[test]
fn the_effects_example_toggles_its_effects() -> TestResult {
    let _entered = Runtime::new().enter();
    let active = signal(false);
    let mut ui = example(860.0, 300.0, move || effects_demo(active));
    let toggle = ui.text_bounds("Toggle effects").ok_or("toggle")?.center();
    ui.click_at(toggle.x, toggle.y);
    ui.run(0.5);
    assert!(active.get_untracked());
    ui.render()?;
    Ok(())
}

#[test]
fn the_motion_example_runs_and_settles() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(760.0, 480.0, motion_demo);
    ui.run(1.0);
    ui.render()?;
    Ok(())
}

#[test]
fn the_diff_example_renders_the_sample_patch() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(1280.0, 800.0, || diff_view(Patch::parse(SAMPLE)));
    assert!(ui.bounds_of("diff-file-0").is_some());
    ui.render()?;
    Ok(())
}

#[test]
fn the_settings_example_switches_tabs_and_checks_for_updates() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(560.0, 520.0, settings);
    let about = ui.text_bounds("About").ok_or("about tab")?.center();
    ui.click_at(about.x, about.y);
    ui.click_on("check")?;
    assert!(ui.text_bounds("Checking…").is_some());
    ui.render()?;
    Ok(())
}

fn example<T: View>(width: f32, height: f32, root: impl FnOnce() -> T) -> TestUi {
    TestUiBuilder::default()
        .size(width, height)
        .fonts(bundled_fonts())
        .build(root)
}
