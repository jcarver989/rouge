mod common;

use common::builder;
use rouge_core::testing::TestResult;
use rouge_core::{
    Chord, EditAction, Key, Menu, MenuItem, Modifiers, Point, PointerButton, Styled, WindowCommand,
    div, editable, focus_handle,
};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{MenuFocus, context_menu, menu_item, select};
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

#[test]
fn nested_context_menus_are_keyboard_reachable_and_close_the_entire_chain() -> TestResult {
    let _rt = Runtime::new().enter();
    let chosen = signal(0);
    let mut ui = builder().size(600.0, 400.0).build(move || {
        context_menu(
            div().size_full(),
            [
                MenuItem::Separator,
                MenuItem::Submenu(Menu::new(
                    "More",
                    vec![menu_item("Nested", move || chosen.set(2))],
                )),
                menu_item("Last", move || chosen.set(3)),
            ],
        )
        .size_full()
    });
    let closed = ui.stats().elements;
    ui.pointer_moved(Point::new(20.0, 20.0));
    ui.pointer_down(PointerButton::Secondary);
    ui.pointer_up(PointerButton::Secondary);
    ui.run(0.2);
    ui.press(Key::ArrowUp);
    ui.press(Key::Enter);
    ui.run(0.2);
    assert_eq!(chosen.get(), 3, "up wraps past the submenu to the last row");
    assert_eq!(ui.stats().elements, closed);

    ui.pointer_down(PointerButton::Secondary);
    ui.pointer_up(PointerButton::Secondary);
    ui.run(0.2);
    ui.press(Key::ArrowRight);
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Nested").is_some());
    ui.press(Key::ArrowLeft);
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Nested").is_none());
    ui.press(Key::Enter);
    ui.run(0.2);
    assert!(
        ui.bounds_of("menu-item-Nested").is_some(),
        "focus returns to the submenu trigger"
    );
    ui.press(Key::Escape);
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Nested").is_none());
    assert!(ui.bounds_of("menu-More").is_some());
    ui.press(Key::ArrowRight);
    ui.run(0.2);
    ui.press(Key::Enter);
    ui.run(0.2);
    assert_eq!(chosen.get(), 2);
    assert_eq!(ui.stats().elements, closed, "a leaf closes every ancestor");
    Ok(())
}

#[test]
fn context_menu_focus_policy_preserves_editor_for_nested_actions_and_callbacks() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal("original".to_owned());
    let called = signal(false);
    let (mut ui, focus) = builder().size(600.0, 400.0).build_with(|| {
        let focus = focus_handle();
        let items = [MenuItem::Submenu(Menu::new(
            "Commands",
            vec![
                MenuItem::action("Select All", EditAction::SelectAll)
                    .shortcut(Chord::new(Key::character("a"), Modifiers::COMMAND)),
                menu_item("Callback", move || called.set(true)),
            ],
        ))];
        (
            context_menu(editable(value).track_focus(focus).w_full(), items)
                .focus_policy(MenuFocus::Preserve)
                .size_full(),
            focus,
        )
    });
    ui.click_at(20.0, 10.0);
    assert!(focus.is_focused());
    for title in ["Callback", "Select All"] {
        ui.pointer_down(PointerButton::Secondary);
        ui.pointer_up(PointerButton::Secondary);
        ui.run(0.2);
        ui.click_on("menu-Commands")?;
        ui.run(0.2);
        ui.click_on(&format!("menu-item-{title}"))?;
        assert!(focus.is_focused());
        for command in ui.take_window_commands() {
            if let WindowCommand::Menu(item) = command {
                assert!(item.dispatch(&mut ui));
            }
        }
        ui.run(0.2);
    }
    assert!(called.get());
    ui.type_text("changed");
    assert_eq!(value.get(), "changed");
    Ok(())
}
