mod common;

use common::{builder, handheld_test, viewport};
use rouge_core::access::{Action, Role};
use rouge_core::testing::{TestResult, TestUi};
use rouge_core::{
    Chord, Div, EditAction, Key, Menu, MenuItem, Modifiers, Point, Styled, Theme, TouchPhase,
    WindowCommand, div, editable, focus_handle, view,
};
use rouge_reactive::{Runtime, Signal, signal};
use rouge_theme::UiPalette;
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::CONTROL_HEIGHT_LARGE;
use rouge_widgets::{MenuFocus, button, context_menu, select};

#[test]
fn a_select_is_a_combo_box_whose_menu_items_choose() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, quality) = select_fixture();
    let closed = ui.access_node(Role::ComboBox, "Quality")?;
    assert_eq!(closed.value.as_deref(), Some("Two"));
    assert_eq!(closed.expanded, Some(false));
    ui.perform(Role::ComboBox, "Quality", Action::Click)?;
    ui.run(0.5);
    assert_eq!(
        ui.access_node(Role::ComboBox, "Quality")?.expanded,
        Some(true)
    );
    assert!(ui.perform(Role::MenuItem, "Three", Action::Click)?);
    ui.run(0.5);
    assert_eq!(quality.get(), 3);
    assert_eq!(
        ui.access_node(Role::ComboBox, "Quality")?.value.as_deref(),
        Some("Three")
    );
    Ok(())
}

#[test]
fn select_opens_a_menu_and_picks_by_click_or_keyboard() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, value) = select_fixture();
    ui.click_on("select")?;
    ui.run(0.6);
    ui.click_on("menu-item-Three")?;
    assert_eq!(value.get(), 3, "the third row");
    ui.run(0.6);
    assert!(
        ui.bounds_of("menu-item-Three").is_none(),
        "choosing closes the menu"
    );
    ui.click_on("select")?;
    ui.run(0.6);
    ui.click_on("menu-item-One")?;
    assert_eq!(value.get(), 1, "the first row");
    ui.run(0.6);

    assert!(
        ui.press("down"),
        "focus returns to the trigger, which opens on arrow down"
    );
    ui.run(0.6);
    ui.press("down down enter");
    assert_eq!(value.get(), 3, "arrow keys walk the rows and enter picks");
    ui.run(0.6);
    ui.press("enter");
    ui.run(0.6);
    ui.press("up escape");
    ui.run(0.6);
    assert_eq!(value.get(), 3, "escape closes without choosing");
    assert!(ui.bounds_of("menu-item-Three").is_none());
    Ok(())
}

#[test]
fn context_menu_opens_at_the_pointer_on_secondary_click() -> TestResult {
    let _entered = Runtime::new().enter();
    let chosen = signal(None);
    let mut ui = builder().build(move || {
        let items = [
            MenuItem::callback("Copy", move || chosen.set(Some("copy"))),
            MenuItem::callback("Paste", move || chosen.set(Some("paste"))),
        ];
        context_menu(viewport().child(context_target()), items).size_full()
    });
    let pointer = ui.center_of("context-target")?;
    ui.click_on("context-target")?;
    assert!(
        ui.bounds_of("menu-item-Copy").is_none(),
        "a primary click does nothing"
    );
    ui.right_click_on("context-target")?;
    ui.run(0.6);
    let row = ui.rect_of("menu-item-Copy")?;
    assert!(
        row.origin.x >= pointer.x && row.origin.y >= pointer.y,
        "starts at the pointer"
    );
    ui.click_on("menu-item-Paste")?;
    ui.run(0.6);
    assert_eq!(chosen.get(), Some("paste"));
    assert!(ui.bounds_of("menu-item-Copy").is_none());
    Ok(())
}

#[test]
fn a_context_menu_near_a_corner_flips_and_shifts_but_remains_clickable() -> TestResult {
    let _entered = Runtime::new().enter();
    let chosen = signal(false);
    let mut ui = builder().build(move || {
        let items = [MenuItem::callback("Copy", move || chosen.set(true))];
        let target = context_target().absolute().right(0.0).bottom(0.0);
        context_menu(viewport().child(target), items).size_full()
    });
    let pointer = ui.center_of("context-target")?;
    ui.right_click_on("context-target")?;
    ui.run(0.6);
    let row = ui.rect_of("menu-item-Copy")?;
    assert!(row.max().y <= pointer.y, "flips above the pointer");
    assert!(row.origin.x < pointer.x, "shifts away from the right edge");
    assert_eq!(ui.visible_rect_of("menu-item-Copy")?, row);
    ui.click_on("menu-item-Copy")?;
    assert!(chosen.get());
    Ok(())
}

#[test]
fn nested_context_menus_are_keyboard_reachable_and_close_the_entire_chain() -> TestResult {
    let _entered = Runtime::new().enter();
    let chosen = signal(0);
    let mut ui = builder().size(600.0, 400.0).build(move || {
        context_menu(
            viewport().child(context_target()),
            [
                MenuItem::Separator,
                MenuItem::Submenu(Menu::new(
                    "More",
                    vec![MenuItem::callback("Nested", move || chosen.set(2))],
                )),
                MenuItem::callback("Last", move || chosen.set(3)),
            ],
        )
        .size_full()
    });
    ui.right_click_on("context-target")?;
    ui.run(0.2);
    ui.press("up");
    ui.press("enter");
    ui.run(0.2);
    assert_eq!(chosen.get(), 3, "up wraps past the submenu to the last row");
    assert!(ui.bounds_of("menu-More").is_none());

    ui.right_click_on("context-target")?;
    ui.run(0.2);
    ui.press("right");
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Nested").is_some());
    ui.press("left");
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Nested").is_none());
    ui.press("enter");
    ui.run(0.2);
    assert!(
        ui.bounds_of("menu-item-Nested").is_some(),
        "focus returns to the submenu trigger"
    );
    ui.press("escape");
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Nested").is_none());
    assert!(ui.bounds_of("menu-More").is_some());
    ui.press("right");
    ui.run(0.2);
    ui.press("enter");
    ui.run(0.2);
    assert_eq!(chosen.get(), 2);
    assert!(
        ui.bounds_of("menu-More").is_none(),
        "a leaf closes every ancestor"
    );
    assert!(ui.bounds_of("menu-item-Nested").is_none());
    Ok(())
}

#[test]
fn context_menu_focus_policy_preserves_editor_for_nested_actions_and_callbacks() -> TestResult {
    let _entered = Runtime::new().enter();
    let value = signal("original".to_owned());
    let called = signal(false);
    let (mut ui, focus) = builder().size(600.0, 400.0).build_with(|| {
        let focus = focus_handle();
        let items = [MenuItem::Submenu(Menu::new(
            "Commands",
            vec![
                MenuItem::action("Select All", EditAction::SelectAll)
                    .shortcut(Chord::new(Key::character("a"), Modifiers::COMMAND)),
                MenuItem::callback("Callback", move || called.set(true)),
            ],
        ))];
        (
            context_menu(
                editable(value)
                    .test_id("editor")
                    .track_focus(focus)
                    .w(160.0),
                items,
            )
            .focus_policy(MenuFocus::Preserve)
            .size_full(),
            focus,
        )
    });
    ui.click_on("editor")?;
    assert!(focus.is_focused());
    for title in ["Callback", "Select All"] {
        ui.right_click_on("editor")?;
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

#[test]
fn menu_flips_and_clamps_at_window_edges_and_activates_by_keyboard() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, chosen) = component_test().size(320.0, 220.0).build_with(|| {
        let chosen = signal(0);
        let items = [
            MenuItem::callback("Edit", move || chosen.set(1)),
            MenuItem::Separator,
            MenuItem::callback("Delete", move || chosen.set(2)).destructive(),
        ];
        let root = view! {
            <div size_full test_id="viewport" flex col items_end justify_end>
                <div test_id="outside" size=20.0 absolute left=0.0 top=0.0/>
                <context_menu items dropdown><button test_id="trigger" label="Menu"/></context_menu>
            </div>
        };
        (root, chosen)
    });
    assert!(ui.bounds_of("menu-item-Edit").is_none());
    ui.click_on("trigger")?;
    ui.run(0.2);
    let trigger = ui.rect_of("trigger")?;
    let entry = ui.rect_of("menu-item-Delete")?;
    assert!(entry.max().y <= trigger.origin.y, "flips above the trigger");
    assert!(
        entry.max().x <= ui.rect_of("viewport")?.max().x,
        "shifts into the window"
    );
    ui.press("end");
    ui.press("enter");
    assert_eq!(chosen.get(), 2);
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Delete").is_none());
    ui.press("enter");
    ui.run(0.2);
    ui.click_on("menu-item-Edit")?;
    assert_eq!(chosen.get(), 1);
    ui.run(0.2);
    ui.press("enter");
    ui.run(0.2);
    ui.click_on("outside")?;
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Edit").is_none());
    Ok(())
}

#[test]
fn menu_items_can_be_given_their_own_test_id() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, chosen) = component_test().size(320.0, 220.0).build_with(|| {
        let chosen = signal(false);
        let items = [
            MenuItem::callback("Archive inactive threads", move || chosen.set(true)).id("archive"),
        ];
        (dropdown(items), chosen)
    });
    ui.click_on("trigger")?;
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Archive inactive threads").is_none());
    ui.click_on("archive")?;
    assert!(chosen.get());
    Ok(())
}

#[test]
fn hovering_a_dropdown_item_highlights_its_row() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(320.0, 220.0).build(|| {
        Theme::provide(UiPalette::dark());
        let items = [
            MenuItem::callback("First", || {}).id("first"),
            MenuItem::callback("Second", || {}).id("second"),
        ];
        dropdown(items)
    });
    ui.click_on("trigger")?;
    ui.run(0.3);
    let row = ui.rect_of("second")?;
    let (x, y) = ((row.origin.x + 4.0) as u32, row.center().y as u32);
    let resting = ui.pixel(x, y)?;
    ui.hover_on("second")?;
    ui.run(0.3);
    let hovered = ui.pixel(x, y)?;
    assert_ne!(resting, hovered, "the hovered row is highlighted");
    Ok(())
}

#[test]
fn group_labels_are_skipped_by_the_keyboard_and_shortcuts_sit_in_a_key_cap() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, chosen) = component_test().size(320.0, 220.0).build_with(|| {
        let chosen = signal(0);
        let items = [
            MenuItem::label("Queue"),
            MenuItem::callback("Play next", move || chosen.set(1)).shortcut("secondary-enter"),
            MenuItem::label("Library"),
            MenuItem::callback("Remove", move || chosen.set(2)).destructive(),
        ];
        (dropdown(items), chosen)
    });
    ui.click_on("trigger")?;
    ui.run(0.3);
    let label = ui.text_bounds("Library").ok_or("the label is drawn")?;
    let first = ui.rect_of("menu-item-Play next")?;
    let last = ui.rect_of("menu-item-Remove")?;
    assert!(first.max().y <= label.origin.y && label.max().y <= last.origin.y);
    ui.press("down");
    ui.press("enter");
    assert_eq!(
        chosen.get(),
        2,
        "arrows step from item to item over the label"
    );
    ui.run(0.3);
    ui.click_on("trigger")?;
    ui.run(0.3);
    chosen.set(0);
    ui.click_text("Library")?;
    ui.run(0.3);
    assert_eq!(chosen.get(), 0, "a label does nothing");
    assert!(
        ui.bounds_of("menu-item-Remove").is_some(),
        "and leaves the menu open"
    );
    Ok(())
}

#[test]
fn select_lists_its_options_in_a_sheet_on_a_handheld() -> TestResult {
    let _entered = Runtime::new().enter();
    let value = signal(1u32);
    let mut ui = handheld_test().build(move || {
        let options = [(1, "One".to_owned()), (2, "Two".to_owned())];
        view! {
            <div size_full p=10.0 test_id="viewport">
                {select_target(value, options)}
            </div>
        }
    });
    ui.tap_on("select")?;
    ui.run(2.0);
    let row = ui.rect_of("menu-item-Two")?;
    let trigger = ui.rect_of("select")?;
    let viewport = ui.rect_of("viewport")?;
    assert_eq!(row.size.height, CONTROL_HEIGHT_LARGE, "a touch-sized row");
    assert!(
        row.origin.y > viewport.center().y,
        "opens at the bottom of the window"
    );
    assert!(
        row.size.width > trigger.size.width,
        "not constrained to the trigger width"
    );
    ui.tap_on("menu-item-Two")?;
    assert_eq!(value.get(), 2);
    ui.run(2.0);
    assert_eq!(
        ui.bounds_of("menu-item-Two"),
        None,
        "choosing closes the sheet"
    );
    Ok(())
}

#[test]
fn on_touch_a_long_press_opens_the_context_menu_and_a_tap_or_a_pan_does_not() -> TestResult {
    let _entered = Runtime::new().enter();
    let (opened, flagged) = (signal(0), signal(false));
    let mut ui = handheld_test()
        .build(move || {
            let items = [MenuItem::callback("Flag", move || flagged.set(true))];
            view! {
                <div size_full test_id="viewport">
                    <context_menu items>
                        <div test_id="row" h=60.0 on_click={move |_| opened.update(|count| *count += 1)}/>
                    </context_menu>
                </div>
            }
        });
    let row = ui.rect_of("row")?;
    let at = row.center();
    let pan = Point::new(at.x, row.max().y + row.size.height);
    ui.tap_on("row")?;
    ui.run(1.0);
    assert_eq!(opened.get(), 1, "a tap is still a tap");
    assert_eq!(ui.bounds_of("menu-item-Flag"), None);
    ui.touch(TouchPhase::Start, at);
    ui.run(0.3);
    ui.touch(TouchPhase::Move, pan);
    ui.run(1.0);
    ui.touch(TouchPhase::End, pan);
    ui.run(0.5);
    assert_eq!(
        ui.bounds_of("menu-item-Flag"),
        None,
        "a pan is not a long press"
    );
    ui.touch(TouchPhase::Start, at);
    ui.run(0.3);
    assert_eq!(ui.bounds_of("menu-item-Flag"), None, "not yet");
    ui.run(1.5);
    let row = ui.rect_of("menu-item-Flag")?;
    assert!(
        row.origin.y > ui.rect_of("viewport")?.center().y,
        "it opens at the bottom as a sheet: {row:?}"
    );
    ui.touch(TouchPhase::End, at);
    ui.run(0.5);
    assert_eq!(
        opened.get(),
        1,
        "lifting the finger does not also tap the row"
    );
    assert!(
        ui.bounds_of("menu-item-Flag").is_some(),
        "or close the menu"
    );
    ui.tap_on("menu-item-Flag")?;
    assert!(flagged.get());
    Ok(())
}

fn dropdown<const N: usize>(items: [MenuItem; N]) -> Div {
    view! {
        <div size_full><context_menu items dropdown><button test_id="trigger" label="Menu"/></context_menu></div>
    }
}

fn select_fixture() -> (TestUi, Signal<u32>) {
    let value = signal(2u32);
    let ui = builder().build(move || {
        let options = [
            (1, "One".to_owned()),
            (2, "Two".to_owned()),
            (3, "Three".to_owned()),
        ];
        viewport().p(10.0).child(select_target(value, options))
    });
    (ui, value)
}

fn context_target() -> Div {
    div().test_id("context-target").size(20.0)
}

fn select_target(value: Signal<u32>, options: impl IntoIterator<Item = (u32, String)>) -> Div {
    div()
        .test_id("select")
        .w(140.0)
        .child(select(value, options).access_label("Quality").w_full())
}
