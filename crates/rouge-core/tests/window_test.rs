use rouge_core::testing::{TestResult, TestUiBuilder};
use rouge_core::{
    Appearance, EditAction, MenuItem, Point, Styled, WindowCommand, appearance, div, editable,
    window_focused, window_handle,
};
use rouge_reactive::{Runtime, signal};

#[test]
fn appearance_and_title_are_reactive_and_window_commands_are_owned_by_each_ui() {
    let _rt = Runtime::new().enter();
    let (mut ui, (window, appearance)) =
        TestUiBuilder::default().build_with(|| (div(), (window_handle(), appearance())));
    assert_eq!(appearance.get(), Appearance::Light);
    ui.set_appearance(Appearance::Dark);
    assert_eq!(appearance.get(), Appearance::Dark);
    window.set_title("Diff");
    window.minimize();
    window.zoom();
    assert_eq!(window.title().get(), "Diff");
    assert!(
        matches!(&ui.take_window_commands()[..], [WindowCommand::SetTitle(title), WindowCommand::Minimize, WindowCommand::Zoom] if title == "Diff")
    );
    assert!(ui.take_window_commands().is_empty());
    drop(ui);
    window.set_title("disposed");
    window.close();
}

#[test]
fn shared_menu_actions_reach_the_focused_editor() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal("selected".to_owned());
    let mut ui = TestUiBuilder::default().build(|| editable(value).w(180.0));
    ui.click(Point::new(15.0, 10.0));
    assert!(MenuItem::action("Select All", EditAction::SelectAll).dispatch(&mut ui));
    ui.type_text("replacement");
    assert_eq!(value.get(), "replacement");
    assert!(!MenuItem::Separator.dispatch(&mut ui));
    Ok(())
}

#[test]
fn menu_callbacks_run_in_the_dispatching_windows_scope() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().build(div);
    let mut other = TestUiBuilder::default().build(div);
    let callback = MenuItem::callback("Rename", || window_handle().set_title("callback"));
    assert!(callback.dispatch(&mut ui));
    assert!(
        matches!(&ui.take_window_commands()[..], [WindowCommand::SetTitle(title)] if title == "callback")
    );
    assert!(other.take_window_commands().is_empty());
    assert!(callback.dispatch(&mut other));
    assert!(
        matches!(&other.take_window_commands()[..], [WindowCommand::SetTitle(title)] if title == "callback")
    );
    assert!(ui.take_window_commands().is_empty());
    Ok(())
}

#[test]
fn window_focus_is_reactive_and_focusing_is_a_window_command() {
    let _rt = Runtime::new().enter();
    let (mut ui, (window, focused)) =
        TestUiBuilder::default().build_with(|| (div(), (window_handle(), window_focused())));
    assert!(focused.get());
    ui.set_focused(false);
    assert!(!focused.get());
    window.focus();
    assert!(matches!(
        &ui.take_window_commands()[..],
        [WindowCommand::Focus]
    ));
    ui.set_focused(true);
    assert!(focused.get());
}

#[test]
fn dispatching_an_action_reaches_the_focused_handlers_through_the_window() {
    let _rt = Runtime::new().enter();
    let pings = signal(0);
    let (mut ui, window) = TestUiBuilder::default().build_with(move || {
        let root = div().on_action(move |_: &Ping| pings.update(|pings| *pings += 1));
        (root, window_handle())
    });
    window.dispatch_action(Ping);
    ui.advance(0.0);
    assert_eq!(pings.get(), 1);
}

#[derive(Clone, Copy)]
struct Ping;
