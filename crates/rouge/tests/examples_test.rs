#![cfg(not(target_arch = "wasm32"))]

#[allow(dead_code)]
#[path = "../examples/shared/mod.rs"]
mod shared;
#[path = "../examples/shared/todo/mod.rs"]
mod todo;

use rouge::core::WindowCommand;
use rouge::core::testing::{TestResult, TestUi, TestUiBuilder};
use rouge::persist::MemoryStorage;
use rouge::prelude::*;
use rouge::widgets::bundled_fonts;
use rouge::widgets::tokens::MotionScale;
use shared::counter_view::counter;
use shared::diff::{Patch, SAMPLE, diff_view};
use shared::effects_view::effects_demo;
use shared::glass_view::glass;
use shared::motion_view::motion_demo;
use shared::settings_view::settings;
use std::error::Error;
use std::rc::Rc;

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

#[test]
fn the_todomvc_example_adds_completes_filters_and_clears() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(640.0, 760.0, todo::todo_app);
    add(&mut ui, "Buy milk");
    add(&mut ui, "  Walk the dog  ");
    add(&mut ui, "   ");
    assert!(ui.text_bounds("2 items left").is_some());
    assert!(ui.text_bounds("Walk the dog").is_some());
    assert!(ui.bounds_of("todo-2").is_none(), "blank titles are ignored");
    ui.click_on("todo-0-toggle")?;
    assert!(ui.text_bounds("1 item left").is_some());
    ui.click_on("filter-active")?;
    ui.run(0.5);
    assert!(ui.bounds_of("todo-0").is_none());
    assert!(ui.bounds_of("todo-1").is_some());
    ui.click_on("filter-all")?;
    ui.click_on("clear-completed")?;
    ui.run(0.5);
    assert!(ui.bounds_of("todo-0").is_none());
    assert!(ui.bounds_of("clear-completed").is_none());
    ui.click_on("toggle-all")?;
    assert!(ui.text_bounds("0 items left").is_some());
    ui.hover_on("todo-1")?;
    ui.run(0.5);
    ui.click_on("todo-1-delete")?;
    ui.run(0.5);
    assert!(
        ui.bounds_of("toggle-all").is_none(),
        "the list and footer hide when empty"
    );
    Ok(())
}

#[test]
fn the_todomvc_rings_follow_the_list_and_not_the_filter() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(640.0, 760.0, todo::todo_app);
    let bare = lower_field(&mut ui)?;
    add(&mut ui, "First proof");
    add(&mut ui, "Second proof");
    ui.run(3.0);
    let open = lower_field(&mut ui)?;
    assert!(open != bare, "each todo draws a ring");
    ui.click_on("filter-completed")?;
    ui.run(1.0);
    assert!(
        lower_field(&mut ui)? == open,
        "filters do not move the rings"
    );
    ui.click_on("filter-all")?;
    ui.click_on("todo-0-toggle")?;
    ui.advance(0.05);
    let inking = lower_field(&mut ui)?;
    ui.run(3.0);
    let inked = lower_field(&mut ui)?;
    assert!(inked != open && inking != open && inking != inked);
    assert_eq!(
        ui.compositor_frames(1.0)?,
        0,
        "the rings are clockless at rest"
    );
    ui.click_on("todo-0-toggle")?;
    ui.run(3.0);
    assert!(
        lower_field(&mut ui)? == open,
        "reopening returns the ring to graphite"
    );
    ui.click_on("toggle-all")?;
    ui.run(3.0);
    ui.click_on("clear-completed")?;
    ui.run(3.0);
    assert!(
        lower_field(&mut ui)? == bare,
        "an empty list leaves bare paper"
    );
    Ok(())
}

#[test]
fn a_todomvc_change_sends_one_wave_that_leaves_no_trace() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(640.0, 760.0, todo::todo_app);
    assert!(
        ui.bounds_of("wave").is_none(),
        "nothing moves before a change"
    );
    add(&mut ui, "Proof");
    assert!(ui.bounds_of("wave").is_some(), "adding sends a wave");
    ui.run(1.0);
    let bent = lower_field(&mut ui)?;
    ui.run(2.0);
    assert!(ui.bounds_of("wave").is_none(), "the wave unmounts");
    assert!(lower_field(&mut ui)? != bent, "the wave bent the rings");
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing draws at rest");
    Ok(())
}

#[test]
fn reduced_motion_and_saved_todos_land_on_the_final_rings() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(640.0, 760.0, || {
        MotionScale::None.provide();
        todo::todo_app()
    });
    add(&mut ui, "Proof");
    let open = lower_field(&mut ui)?;
    ui.click_on("todo-0-toggle")?;
    let inked = lower_field(&mut ui)?;
    ui.run(1.0);
    assert!(inked != open && inked == lower_field(&mut ui)?);
    assert!(
        ui.bounds_of("wave").is_none(),
        "reduced motion sends no wave"
    );
    let storage = MemoryStorage::default();
    storage.insert("todos", r#"[{"id":0,"title":"Proof","done":true}]"#);
    let mut restored = saved_todos(&storage);
    assert!(
        restored.bounds_of("wave").is_none(),
        "restoring sends no wave"
    );
    restored.run(1.0);
    assert!(
        lower_field(&mut restored)? == inked,
        "saved todos restore the same rings"
    );
    Ok(())
}

#[test]
fn the_todomvc_filter_follows_the_route() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(640.0, 760.0, todo::todo_app);
    add(&mut ui, "Read");
    add(&mut ui, "Write");
    ui.click_on("todo-1-toggle")?;
    ui.take_window_commands();
    ui.click_on("filter-completed")?;
    assert!(
        matches!(&ui.take_window_commands()[..], [WindowCommand::Navigate(path)] if path == "/completed")
    );
    ui.run(0.5);
    assert!(ui.bounds_of("todo-0").is_none());
    ui.set_route("/active");
    ui.run(0.5);
    assert!(ui.bounds_of("todo-0").is_some());
    assert!(ui.bounds_of("todo-1").is_none());
    Ok(())
}

#[test]
fn the_todomvc_example_edits_with_commit_cancel_and_delete_on_empty() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(640.0, 760.0, todo::todo_app);
    add(&mut ui, "Buy milk");
    add(&mut ui, "Walk the dog");
    ui.double_click_on("todo-0-title")?;
    ui.type_text("Buy oat milk");
    ui.press("enter");
    assert!(ui.text_bounds("Buy oat milk").is_some());
    ui.double_click_on("todo-0-title")?;
    ui.type_text("Something else");
    ui.press("escape");
    assert!(ui.text_bounds("Buy oat milk").is_some());
    ui.double_click_on("todo-1-title")?;
    ui.type_text("Walk the cat");
    ui.click_on("todo-0-title")?;
    assert!(
        ui.text_bounds("Walk the cat").is_some(),
        "leaving the editor commits it"
    );
    ui.double_click_on("todo-1-title")?;
    ui.press("backspace enter");
    ui.run(0.5);
    assert!(
        ui.bounds_of("todo-1").is_none(),
        "an empty title deletes the todo"
    );
    Ok(())
}

#[test]
fn the_todomvc_example_restores_saved_todos() -> TestResult {
    let _entered = Runtime::new().enter();
    let storage = MemoryStorage::default();
    storage.insert(
        "todos",
        r#"[{"id":4,"title":"Saved","done":true},{"id":7,"title":"Open","done":false}]"#,
    );
    let mut ui = saved_todos(&storage);
    assert!(ui.text_bounds("Saved").is_some());
    assert!(ui.text_bounds("1 item left").is_some());
    add(&mut ui, "New");
    assert!(
        ui.bounds_of("todo-8").is_some(),
        "ids continue after the saved ones"
    );
    drop(ui);
    let saved = storage.get("todos").ok_or("saved on close")?;
    assert!(saved.contains("\"New\""));
    Ok(())
}

#[test]
fn the_todomvc_footer_stacks_inside_a_phone_width_card() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(402.0, 874.0, todo::todo_app);
    add(&mut ui, "Buy milk");
    ui.click_on("todo-0-toggle")?;
    ui.run(0.5);
    let card = ui.rect_of("card")?;
    let clear = ui.rect_of("clear-completed")?;
    let items_left = ui.rect_of("items-left")?;
    let tabs = ui.rect_of("filter-all")?;
    assert!(
        clear.max().x <= card.max().x,
        "clear completed stays in the card"
    );
    assert!(
        tabs.max().y <= items_left.origin.y,
        "the filter tabs sit above the counts"
    );
    assert!(
        items_left.size.height <= clear.size.height,
        "the count fits on one line"
    );
    Ok(())
}

#[test]
fn the_todomvc_footer_is_one_row_on_a_wide_card() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = example(640.0, 760.0, todo::todo_app);
    add(&mut ui, "Buy milk");
    ui.click_on("todo-0-toggle")?;
    ui.run(0.5);
    let card = ui.rect_of("card")?;
    let clear = ui.rect_of("clear-completed")?;
    let tabs = ui.rect_of("filter-all")?;
    assert!(clear.max().x <= card.max().x);
    assert_eq!(tabs.center().y, clear.center().y);
    Ok(())
}

#[test]
fn toggling_one_todo_costs_the_same_in_a_long_list() -> TestResult {
    let _entered = Runtime::new().enter();
    let short = toggle_work(50)?;
    assert!(short[0] > 0, "the toggle ran its effects");
    assert_eq!(short, toggle_work(500)?);
    Ok(())
}

fn add(ui: &mut TestUi, title: &str) {
    ui.type_text(title);
    ui.press("enter");
}

fn saved_todos(storage: &MemoryStorage) -> TestUi {
    let storage = storage.clone();
    let mut ui = example(640.0, 760.0, move || {
        provide_storage(Rc::new(storage));
        todo::todo_app()
    });
    ui.advance(0.0);
    ui
}

fn toggle_work(count: u64) -> Result<[u64; 4], Box<dyn Error>> {
    let storage = MemoryStorage::default();
    let todos: Vec<String> = (0..count)
        .map(|id| format!(r#"{{"id":{id},"title":"Todo {id}","done":false}}"#))
        .collect();
    storage.insert("todos", format!("[{}]", todos.join(",")));
    let mut ui = saved_todos(&storage);
    ui.click_on("todo-0-toggle")?;
    ui.run(0.5);
    let before = ui.stats();
    ui.click_on("todo-0-toggle")?;
    let work = ui.stats() - before;
    Ok([
        work.reactive.effects_run,
        work.reactive.memos_run,
        work.layer_paints,
        work.layouts,
    ])
}

fn example<T: View>(width: f32, height: f32, root: impl FnOnce() -> T) -> TestUi {
    TestUiBuilder::default()
        .size(width, height)
        .fonts(bundled_fonts())
        .build(root)
}

fn lower_field(ui: &mut TestUi) -> Result<Vec<u8>, Box<dyn Error>> {
    let image = ui.render()?;
    let start = (image.height * 3 / 4 * image.width * 4) as usize;
    Ok(image.pixels[start..].to_vec())
}
