mod common;

use common::builder;
use rouge_core::{Key, Styled, div};
use rouge_reactive::Runtime;
use rouge_widgets::RovingFocus;

#[test]
fn arrow_home_and_end_keys_move_focus_between_items_and_wrap() {
    let _entered = Runtime::new().enter();
    let (mut ui, focus) = builder().build_with(|| {
        let focus = RovingFocus::new(3);
        let rows = (0..3).map(|index| focus.item(index, div().h(10.0).focusable()));
        (div().size_full().children(rows.collect::<Vec<_>>()), focus)
    });
    ui.advance(0.0);
    let focused =
        |focus: &RovingFocus| (0..focus.len()).find(|index| focus.handle(*index).is_focused());
    assert_eq!(focused(&focus), Some(0), "the first item takes focus");
    ui.press(Key::ArrowDown);
    assert_eq!(focused(&focus), Some(1));
    ui.press(Key::End);
    assert_eq!(focused(&focus), Some(2));
    ui.press(Key::ArrowDown);
    assert_eq!(focused(&focus), Some(0), "moving past the end wraps");
    ui.press(Key::ArrowUp);
    assert_eq!(focused(&focus), Some(2));
    ui.press(Key::Home);
    assert_eq!(focused(&focus), Some(0));
}
