use rouge::App;
use rouge::core::testing::{TestResult, TestUiBuilder};
use rouge::core::{Styled, WindowCommand, div};
use rouge::reactive::Runtime;

#[test]
fn quit_can_be_called_from_an_event_callback_without_capturing_context() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default()
        .build(|| div().size_full().test_id("quit").on_click(|_| App::quit()));
    ui.click_on("quit")?;
    assert!(matches!(
        &ui.take_window_commands()[..],
        [WindowCommand::Quit]
    ));
    Ok(())
}
