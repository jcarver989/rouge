mod common;

use common::builder;
use rouge_core::testing::TestResult;
use rouge_core::{Menu, MenuItem, Point, Styled, WindowCommand, div, editable, focus_handle};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::menu_bar;

#[test]
fn application_menu_preserves_editor_focus_and_dispatches_the_shared_action() -> TestResult {
    let _entered = Runtime::new().enter();
    let value = signal("original".to_owned());
    let (mut ui, focus) = builder().build_with(|| {
        let focus = focus_handle();
        (
            div()
                .size_full()
                .flex()
                .col()
                .child(menu_bar(vec![Menu::edit()]))
                .child(editable(value).track_focus(focus).w_full()),
            focus,
        )
    });
    ui.click(Point::new(20.0, 40.0));
    assert!(focus.is_focused());
    ui.click_on("menu-Edit")?;
    ui.run(0.2);
    ui.click_on("menu-item-Select All")?;
    assert!(focus.is_focused());
    for command in ui.take_window_commands() {
        if let WindowCommand::Menu(item) = command {
            assert!(item.dispatch(&mut ui));
        }
    }
    ui.type_text("changed");
    assert_eq!(value.get(), "changed");
    Ok(())
}

#[test]
fn application_submenus_share_callbacks_and_close_the_parent() -> TestResult {
    let _entered = Runtime::new().enter();
    let called = signal(false);
    let mut ui = builder().size(600.0, 400.0).build(|| {
        menu_bar(vec![Menu::new(
            "File",
            vec![MenuItem::Submenu(Menu::new(
                "More",
                vec![MenuItem::callback("Run", move || called.set(true))],
            ))],
        )])
    });
    let closed = ui.stats().elements;
    ui.click_on("menu-File")?;
    ui.run(0.2);
    ui.click_on("menu-More")?;
    ui.run(0.2);
    ui.click_on("menu-item-Run")?;
    ui.run(0.2);
    assert!(called.get());
    assert_eq!(ui.stats().elements, closed);
    Ok(())
}
