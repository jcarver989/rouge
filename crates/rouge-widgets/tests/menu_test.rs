mod common;

use common::builder;
use rouge_core::testing::TestResult;
use rouge_core::{Key, Point, PointerButton, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{context_menu, menu_item, select};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn select_opens_a_menu_and_picks_by_click_or_keyboard() {
    let _entered = Runtime::new().enter();
    let value = signal(2u32);
    let mut ui = builder().build(move || {
        let options = [
            (1, "One".to_owned()),
            (2, "Two".to_owned()),
            (3, "Three".to_owned()),
        ];
        div()
            .size_full()
            .p(10.0)
            .child(select(value, options).w(140.0))
    });
    let closed = ui.stats().elements;
    ui.click_at(30.0, 22.0);
    ui.run(0.6);
    assert!(ui.stats().elements > closed, "the menu is mounted");
    ui.click_at(30.0, 128.0);
    assert_eq!(value.get(), 3, "the third row");
    ui.run(0.6);
    assert_eq!(ui.stats().elements, closed, "choosing closes the menu");
    ui.click_at(30.0, 22.0);
    ui.run(0.6);
    ui.click_at(30.0, 68.0);
    assert_eq!(value.get(), 1, "the first row");
    ui.run(0.6);

    assert!(
        ui.press(Key::ArrowDown),
        "the trigger has focus again and opens on arrow down"
    );
    ui.run(0.6);
    ui.press(Key::ArrowDown);
    ui.press(Key::ArrowDown);
    ui.press(Key::Enter);
    assert_eq!(value.get(), 3, "arrow keys walk the rows and enter picks");
    ui.run(0.6);
    ui.press(Key::Enter);
    ui.run(0.6);
    ui.press(Key::ArrowUp);
    ui.press(Key::Escape);
    ui.run(0.6);
    assert_eq!(
        (value.get(), ui.stats().elements),
        (3, closed),
        "escape closes without choosing"
    );
}

#[test]
fn context_menu_opens_at_the_pointer_on_secondary_click() -> TestResult {
    let _entered = Runtime::new().enter();
    let chosen = Rc::new(RefCell::new(Vec::new()));
    let (copy, paste) = (chosen.clone(), chosen.clone());
    let mut ui = builder().build(move || {
        let items = [
            menu_item("Copy", move || copy.borrow_mut().push("copy")),
            menu_item("Paste", move || paste.borrow_mut().push("paste")),
        ];
        context_menu(div().size_full(), items).size_full()
    });
    let closed = ui.stats().elements;
    ui.click_at(100.0, 80.0);
    assert_eq!(ui.stats().elements, closed, "a primary click does nothing");
    ui.pointer_moved(Point::new(100.0, 80.0));
    ui.pointer_down(PointerButton::Secondary);
    ui.pointer_up(PointerButton::Secondary);
    ui.run(0.6);
    let row = ui.bounds(110.0, 92.0)?;
    assert!(
        row.origin.x >= 100.0 && row.origin.y >= 80.0,
        "the menu starts at the pointer: {row:?}"
    );
    ui.click_at(110.0, row.origin.y + row.size.height + 10.0);
    ui.run(0.6);
    assert_eq!(*chosen.borrow(), ["paste"]);
    assert_eq!(ui.stats().elements, closed);
    Ok(())
}
