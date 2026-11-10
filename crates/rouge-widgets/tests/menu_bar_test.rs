mod common;

use common::builder;
use rouge_core::testing::TestResult;
use rouge_core::{Menu, Styled, WindowCommand, div, editable, focus_handle};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::menu_bar;

#[test]
fn application_menu_preserves_editor_focus_and_dispatches_the_shared_action() -> TestResult {
    let _rt = Runtime::new().enter();
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
    ui.click(rouge_core::Point::new(20.0, 40.0));
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
