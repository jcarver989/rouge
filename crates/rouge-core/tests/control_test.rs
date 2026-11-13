use rouge_core::testing::{
    BLUE, LINEAR, RED, TestResult, TestUi, TestUiBuilder, assert_near, counter,
};
use rouge_core::{
    Color, EditAction, Key, Point, Pose, Styled, View, div, dynamic, each, editable, focus_handle,
    overlay, show, switch,
};
use rouge_reactive::{ReadSignal, Runtime, Signal, on_cleanup, signal};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn dynamic_replacement_with_a_fresh_focus_handle_does_not_steal_focus() -> TestResult {
    let _rt = Runtime::new().enter();
    let replace = signal(false);
    let (mut ui, original) = TestUiBuilder::default().build_with(|| {
        let original = focus_handle();
        let branch = dynamic(move || {
            let handle = if replace.get() {
                focus_handle()
            } else {
                original
            };
            editable(signal("field".to_owned()))
                .track_focus(handle)
                .w(150.0)
                .test_id("field")
        });
        (div().size_full().child(branch), original)
    });
    ui.click_on("field")?;
    assert!(original.is_focused());
    replace.set(true);
    ui.advance(0.0);
    assert!(!original.is_focused());
    assert!(!ui.has_focus());
    Ok(())
}

#[test]
fn nested_dynamic_replacement_restores_handles_but_genuine_unmount_does_not() -> TestResult {
    let _rt = Runtime::new().enter();
    let version = signal(0);
    let visible = signal(true);
    let value = signal("abc".to_owned());
    let (mut ui, focus) = TestUiBuilder::default().build_with(|| {
        let focus = focus_handle();
        let outer = dynamic(move || {
            let generation = version.get();
            div().child(show(
                move || visible.get(),
                move || {
                    div().child(dynamic(move || {
                        editable(value)
                            .track_focus(focus)
                            .w(150.0 + generation as f32)
                            .test_id("nested")
                    }))
                },
            ))
        });
        (div().size_full().child(outer), focus)
    });
    ui.click_on("nested")?;
    ui.press(Key::End);
    ui.press(Key::ArrowLeft);
    version.set(1);
    version.set(2);
    ui.advance(0.0);
    assert!(focus.is_focused());
    ui.type_text("!");
    assert_eq!(value.get(), "ab!c");
    visible.set(false);
    ui.advance(0.0);
    assert!(!focus.is_focused());
    assert!(!ui.has_focus());
    visible.set(true);
    ui.advance(0.0);
    assert!(
        !focus.is_focused(),
        "a later mount is not an in-place replacement"
    );
    Ok(())
}

#[test]
fn dynamic_replacement_reuses_focus_identity_editor_selection_and_history() -> TestResult {
    let _rt = Runtime::new().enter();
    let version = signal(false);
    let value = signal("hello".to_owned());
    let (mut ui, focus) = TestUiBuilder::default().build_with(|| {
        let focus = focus_handle();
        let branch = dynamic(move || {
            let width = if version.get() { 180.0 } else { 150.0 };
            editable(value)
                .track_focus(focus)
                .w(width)
                .test_id("editor")
        });
        (div().size_full().child(branch), focus)
    });
    ui.click_on("editor")?;
    ui.press(Key::End);
    ui.press(Key::ArrowLeft);
    ui.type_text("!");
    assert_eq!(value.get(), "hell!o");
    let caret = ui.text_cursor_area().ok_or("caret")?;
    version.set(true);
    ui.advance(0.0);
    assert!(focus.is_focused());
    assert_eq!(ui.text_cursor_area(), Some(caret));
    ui.type_text("?");
    assert_eq!(value.get(), "hell!?o");
    ui.dispatch_action(EditAction::Undo);
    assert_eq!(value.get(), "hell!o");
    ui.dispatch_action(EditAction::Undo);
    assert_eq!(value.get(), "hello");
    Ok(())
}

#[test]
fn show_mounts_and_unmounts_in_place() {
    let _rt = Runtime::new().enter();
    let visible = signal(false);
    let mut ui = TestUi::new(|| {
        div()
            .flex()
            .size_full()
            .child(div().w(10.0).h(10.0))
            .child(show(move || visible.get(), || div().w(20.0).h(10.0)))
            .child(div().w(30.0).h(10.0))
    });
    assert_eq!(
        ui.stats().elements,
        5,
        "root + container + 2 boxes + anchor"
    );
    assert_eq!(row_widths(&mut ui), [10.0, 30.0], "anchor takes no space");

    visible.set(true);
    ui.update();
    assert_eq!(ui.stats().elements, 6);
    assert_eq!(
        row_widths(&mut ui),
        [10.0, 20.0, 30.0],
        "mounted between siblings"
    );

    visible.set(false);
    ui.update();
    assert_eq!(ui.stats().elements, 5);
    assert_eq!(row_widths(&mut ui), [10.0, 30.0]);
}

#[test]
fn show_builds_its_branch_only_when_the_condition_flips() {
    let _rt = Runtime::new().enter();
    let (builds, bump) = counter();
    let n = signal(0);
    let width = signal(10.0f32);
    let _ui = TestUi::new(move || {
        let branch = move || {
            bump();
            div().w(width.get())
        };
        div().child(show(move || n.get() > 2, branch))
    });
    for i in 1..=10 {
        n.set(i);
    }
    assert_eq!(builds.get(), 1, "built once when crossing the threshold");
    width.set(20.0);
    assert_eq!(
        builds.get(),
        1,
        "signals read while building do not remount"
    );
}

#[test]
fn show_otherwise_swaps_branches() {
    let _rt = Runtime::new().enter();
    let on = signal(true);
    let mut ui = TestUi::new(|| {
        let branches =
            show(move || on.get(), || div().w(40.0).h(10.0)).otherwise(|| div().w(70.0).h(10.0));
        div().flex().size_full().child(branches)
    });
    assert_eq!(row_widths(&mut ui), [40.0]);
    on.set(false);
    ui.update();
    assert_eq!(row_widths(&mut ui), [70.0]);
    assert_eq!(ui.stats().elements, 4);
}

#[test]
fn show_disposes_reactive_state_of_hidden_branch() {
    let _rt = Runtime::new().enter();
    let (effect_runs, bump) = counter();
    let visible = signal(true);
    let color = signal(Color::RED);
    let _ui = TestUi::new(move || {
        let branch = move || {
            let bump = bump.clone();
            div().bg(move || {
                bump();
                color.get()
            })
        };
        div().child(show(move || visible.get(), branch))
    });
    assert_eq!(effect_runs.get(), 1);
    color.set(Color::BLUE);
    assert_eq!(effect_runs.get(), 2);
    visible.set(false);
    color.set(Color::GREEN);
    assert_eq!(
        effect_runs.get(),
        2,
        "binding of the unmounted branch no longer runs"
    );
}

#[test]
fn each_builds_disposes_moves_and_updates_only_the_rows_that_changed() {
    let _rt = Runtime::new().enter();
    let log = Log::default();
    let (mut ui, items) = list(log.clone());
    assert_eq!(row_widths(&mut ui), [5.0, 10.0, 20.0, 30.0, 7.0]);
    assert_eq!(*log.borrow(), ["build 1", "build 2", "build 3"]);

    let steps = [
        (vec![item(3, 30.0), item(1, 10.0), item(2, 20.0)], vec![]),
        (
            vec![item(4, 40.0), item(1, 10.0), item(3, 30.0)],
            vec!["build 4", "dispose 2"],
        ),
        (vec![item(4, 40.0), item(1, 55.0), item(3, 30.0)], vec![]),
    ];
    let mut paints = 0;
    for (rows, built) in steps {
        log.borrow_mut().clear();
        paints = ui.stats().paints;
        items.set(rows.clone());
        ui.update();
        let widths: Vec<f32> = rows.iter().map(|row| row.width).collect();
        assert_eq!(row_widths(&mut ui), [&[5.0], &widths[..], &[7.0]].concat());
        assert_eq!(*log.borrow(), built, "reordered and kept rows are reused");
    }
    assert_eq!(ui.stats().paints, paints + 1, "a kept row updates in place");
    items.set(vec![item(4, 40.0), item(1, 55.0), item(3, 30.0)]);
    assert!(
        !ui.needs_frame(),
        "an identical list changes nothing at all"
    );

    items.set(Vec::new());
    ui.update();
    assert_eq!(row_widths(&mut ui), [5.0, 7.0]);
    assert_eq!(
        ui.stats().elements,
        5,
        "root + container + 2 static boxes + anchor"
    );
}

#[test]
fn nested_control_flow_keeps_order() {
    let _rt = Runtime::new().enter();
    let items = signal(vec![item(1, 10.0), item(2, 20.0), item(3, 30.0)]);
    let hidden = signal(0u32);
    let mut ui = TestUi::new(|| {
        let row = move |item: ReadSignal<Item>| {
            let Item { id, width } = item.get_untracked();
            show(move || hidden.get() != id, move || div().w(width).h(10.0))
        };
        div()
            .flex()
            .size_full()
            .child(each(move || items.get(), |item| item.id, row))
    });
    assert_eq!(row_widths(&mut ui), [10.0, 20.0, 30.0]);
    hidden.set(2);
    ui.update();
    assert_eq!(row_widths(&mut ui), [10.0, 30.0]);
    items.set(vec![item(3, 30.0), item(2, 20.0), item(1, 10.0)]);
    ui.update();
    assert_eq!(
        row_widths(&mut ui),
        [30.0, 10.0],
        "reordered with one row hidden"
    );
    hidden.set(0);
    ui.update();
    assert_eq!(
        row_widths(&mut ui),
        [30.0, 20.0, 10.0],
        "hidden row reappears in its new position"
    );
    items.set(vec![item(2, 20.0)]);
    ui.update();
    assert_eq!(row_widths(&mut ui), [20.0]);
    assert_eq!(
        ui.stats().elements,
        5,
        "root + container + each anchor + row anchor + row content"
    );
}

#[test]
fn each_inside_show_is_fully_removed() {
    let _rt = Runtime::new().enter();
    let visible = signal(true);
    let items = signal(vec![item(1, 10.0), item(2, 20.0)]);
    let mut ui = TestUi::new(|| {
        let rows = move || {
            each(
                move || items.get(),
                |i| i.id,
                |i| div().h(10.0).w(i.get_untracked().width),
            )
        };
        div()
            .flex()
            .size_full()
            .child(show(move || visible.get(), rows))
    });
    assert_eq!(row_widths(&mut ui), [10.0, 20.0]);
    visible.set(false);
    ui.update();
    assert_eq!(row_widths(&mut ui), Vec::<f32>::new());
    assert_eq!(ui.stats().elements, 3, "root + container + show anchor");
    visible.set(true);
    ui.update();
    assert_eq!(row_widths(&mut ui), [10.0, 20.0]);
}

#[test]
fn dynamic_replaces_content_when_dependencies_change() {
    let _rt = Runtime::new().enter();
    let mode = signal(0u32);
    let mut ui = TestUi::new(|| {
        div()
            .flex()
            .size_full()
            .child(dynamic(move || match mode.get() {
                0 => div().w(10.0).h(10.0).into_any(),
                1 => div().w(20.0).h(10.0).child(div()).into_any(),
                _ => show(|| true, || div().w(30.0).h(10.0)).into_any(),
            }))
    });
    for (next, width) in [(1, 20.0), (2, 30.0), (0, 10.0)] {
        mode.set(next);
        ui.update();
        assert_eq!(row_widths(&mut ui), [width]);
    }
    assert_eq!(
        ui.stats().elements,
        4,
        "previous content is removed each time"
    );
}

#[test]
fn handlers_in_dynamic_content_work_and_can_unmount_themselves() {
    let _rt = Runtime::new().enter();
    let open = signal(true);
    let mut ui = TestUi::new(|| {
        let content = move || div().size(50.0).on_click(move |_| open.set(false));
        div().size_full().child(show(move || open.get(), content))
    });
    ui.click(Point::new(10.0, 10.0));
    assert!(!open.get_untracked());
    ui.update();
    assert_eq!(ui.stats().elements, 3);
    ui.click(Point::new(10.0, 10.0));
}

#[test]
fn show_enter_animates_in_from_a_pose() -> TestResult {
    let _rt = Runtime::new().enter();
    let visible = signal(false);
    let mut ui = TestUi::new(move || {
        let branch = || div().size(20.0).bg(Color::RED);
        let fade =
            show(move || visible.get(), branch).enter(Pose::new().translate_x(100.0), LINEAR);
        div().size_full().child(fade)
    });
    assert!(!ui.is_animating());
    visible.set(true);
    ui.advance(0.0);
    assert_eq!(
        ui.stats().layers,
        2,
        "the branch was promoted to a layer up front"
    );
    assert_eq!(ui.first_x(RED, 5)?, 100, "starts at the enter pose");
    ui.run(0.5);
    assert_eq!(ui.first_x(RED, 5)?, 50);
    ui.run(0.55);
    assert_eq!(ui.first_x(RED, 5)?, 0, "ends at its declared position");
    assert!(!ui.needs_frame());
    Ok(())
}

#[test]
fn show_exit_disposes_immediately_but_keeps_painting_until_the_animation_ends() -> TestResult {
    let _rt = Runtime::new().enter();
    let visible = signal(true);
    let (disposed, bump) = counter();
    let mut ui = TestUi::new(move || {
        let bump = bump.clone();
        let branch = move || {
            on_cleanup(bump.clone());
            div().w(20.0).h(10.0).bg(Color::RED)
        };
        let leaving =
            show(move || visible.get(), branch).exit(Pose::new().translate_x(100.0), LINEAR);
        div()
            .flex()
            .size_full()
            .child(leaving)
            .child(div().w(30.0).h(10.0))
    });
    assert_eq!(row_widths(&mut ui), [20.0, 30.0]);

    visible.set(false);
    assert_eq!(disposed.get(), 1, "reactive state is gone right away");
    ui.advance(0.0);
    assert_eq!(ui.stats().elements, 4, "and so is the element");
    assert_eq!(
        row_widths(&mut ui),
        [30.0],
        "siblings take its place in layout"
    );
    assert_eq!(ui.first_x(RED, 5)?, 0, "but its layer is still on screen");
    ui.run(0.5);
    assert_eq!(ui.first_x(RED, 5)?, 50, "running its exit animation");
    ui.run(0.55);
    assert_eq!(ui.first_x(RED, 5)?, 200, "until it is done");
    assert!(!ui.needs_frame());
    Ok(())
}

#[test]
fn each_rows_enter_and_exit() -> TestResult {
    let _rt = Runtime::new().enter();
    let items = signal(vec![1u32]);
    let mut ui = TestUi::new(move || {
        let row = |_| div().absolute().size(20.0).bg(Color::RED);
        let rows = each(move || items.get(), |id| *id, row)
            .enter(Pose::new().opacity(0.0), LINEAR)
            .exit(Pose::new().translate_x(100.0), LINEAR);
        div().size_full().child(rows)
    });
    assert!(ui.is_animating(), "initial rows enter too");
    ui.run(1.25);
    assert!(!ui.is_animating());
    assert_eq!(ui.first_x(RED, 5)?, 0);

    items.set(Vec::new());
    ui.advance(0.0);
    assert_eq!(ui.stats().elements, 3, "root, container and anchor");
    ui.run(0.5);
    assert_eq!(ui.first_x(RED, 5)?, 50, "the removed row is on its way out");
    ui.run(0.55);
    assert_eq!(ui.first_x(RED, 5)?, 200);
    Ok(())
}

#[test]
fn overlays_mount_in_the_window_overlay_root_above_everything_and_leave_with_their_owner()
-> TestResult {
    let _rt = Runtime::new().enter();
    let open = signal(true);
    let (under, hit_under) = counter();
    let (over, hit_over) = counter();
    let mut ui = TestUi::new(move || {
        let popup = move || {
            let hit_over = hit_over.clone();
            overlay(move || {
                div()
                    .absolute()
                    .left(100.0)
                    .top(0.0)
                    .size(50.0)
                    .bg(Color::BLUE)
                    .on_click(move |_| hit_over())
            })
        };
        div()
            .size_full()
            .child(
                div()
                    .w(20.0)
                    .h(20.0)
                    .overflow_hidden()
                    .child(show(move || open.get(), popup)),
            )
            .child(
                div()
                    .absolute()
                    .inset(0.0)
                    .bg(Color::RED)
                    .on_click(move |_| hit_under()),
            )
    });
    ui.advance(0.0);
    let img = ui.render()?;
    assert_near(
        img.pixel(120, 20),
        BLUE,
        "drawn above later siblings and outside its clipping parent",
    );
    assert_near(
        img.pixel(20, 20),
        RED,
        "the overlay root itself draws nothing",
    );
    ui.click(Point::new(120.0, 20.0));
    ui.click(Point::new(20.0, 20.0));
    assert_eq!(
        (over.get(), under.get()),
        (1, 1),
        "clicks outside the overlay pass through to the page"
    );
    open.set(false);
    ui.advance(0.0);
    assert_near(
        ui.pixel(120, 20)?,
        RED,
        "removed with the scope that created it",
    );
    ui.click(Point::new(120.0, 20.0));
    assert_eq!((over.get(), under.get()), (1, 2));
    Ok(())
}

fn item(id: u32, width: f32) -> Item {
    Item { id, width }
}

fn list(log: Log) -> (TestUi, Signal<Vec<Item>>) {
    let items = signal(vec![item(1, 10.0), item(2, 20.0), item(3, 30.0)]);
    let ui = TestUi::new(move || {
        let row = move |item: ReadSignal<Item>| {
            let id = item.get_untracked().id;
            log.borrow_mut().push(format!("build {id}"));
            let log = log.clone();
            on_cleanup(move || log.borrow_mut().push(format!("dispose {id}")));
            div().h(10.0).w(move || item.get().width)
        };
        div()
            .flex()
            .size_full()
            .child(div().w(5.0).h(10.0))
            .child(each(move || items.get(), |item| item.id, row))
            .child(div().w(7.0).h(10.0))
    });
    (ui, items)
}

fn row_widths(ui: &mut TestUi) -> Vec<f32> {
    let mut widths = Vec::new();
    let mut x = 0.5;
    while let Some(b) = ui
        .bounds_at(Point::new(x, 5.0))
        .filter(|b| b.size.height == 10.0)
    {
        widths.push(b.size.width);
        x = b.origin.x + b.size.width + 0.5;
    }
    widths
}

type Log = Rc<RefCell<Vec<String>>>;

#[derive(Clone, PartialEq, Debug)]
struct Item {
    id: u32,
    width: f32,
}

#[test]
fn an_overlay_opened_inside_another_overlay_sits_above_it() {
    let _rt = Runtime::new().enter();
    let (mut ui, clicked) = TestUiBuilder::default().size(100.0, 100.0).build_with(|| {
        let clicked = signal(Vec::new());
        let inner = move || {
            div()
                .absolute()
                .left(10.0)
                .top(10.0)
                .size(20.0)
                .on_click(move |_| clicked.update(|all| all.push("inner")))
        };
        let outer = move || {
            div()
                .absolute()
                .inset(0.0)
                .on_click(move |_| clicked.update(|all| all.push("outer")))
                .child(overlay(inner))
        };
        (div().size_full().child(overlay(outer)), clicked)
    });
    ui.click_at(15.0, 15.0);
    ui.click_at(60.0, 60.0);
    assert_eq!(clicked.get(), ["inner", "outer"]);
}

#[test]
fn switch_rebuilds_only_when_its_key_changes_and_builds_untracked() -> TestResult {
    let _rt = Runtime::new().enter();
    let tab = signal(0);
    let read = signal(0);
    let (builds, bump) = counter();
    let mut ui = TestUi::new(move || {
        let content = switch(
            move || tab.get() % 2,
            move |tab| {
                bump();
                read.get();
                div().test_id(format!("tab-{tab}")).size(10.0)
            },
        );
        div().size_full().child(content)
    });
    assert!(ui.bounds_of("tab-0").is_some());
    read.set(1);
    tab.set(2);
    ui.advance(0.0);
    assert_eq!(
        builds.get(),
        1,
        "reads in the view and same-key writes do not rebuild"
    );
    tab.set(3);
    ui.advance(0.0);
    assert!(ui.bounds_of("tab-1").is_some());
    assert_eq!(builds.get(), 2);
    Ok(())
}
