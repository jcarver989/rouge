use rouge_core::access::{Action, ActionData, Decrement, Increment, Live, Role, Toggled};
use rouge_core::testing::{AccessNode, TestError, TestResult, TestUi, TestUiBuilder};
use rouge_core::{
    Div, Point, Rect, ScrollState, Scrollable, Styled, div, editable, scroll, show, text, view,
};
use rouge_reactive::{Runtime, signal};

#[test]
fn elements_expose_roles_names_and_values() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        view! {
            <div>
                <text heading_level=1>"Settings"</text>
                <text>"Plain words"</text>
                {button("Save", || {})}
                <div role=Role::Image access_label="Logo" access_description="Ours"/>
            </div>
        }
    });
    let nodes = ui.access_nodes();
    assert_eq!(
        outline(&nodes),
        [
            (Role::Heading, "Settings"),
            (Role::Label, ""),
            (Role::Button, "Save"),
            (Role::Label, ""),
            (Role::Image, "Logo"),
        ]
    );
    assert_eq!(nodes[1].value.as_deref(), Some("Plain words"));
    assert_eq!(nodes[4].description.as_deref(), Some("Ours"));
    Ok(())
}

#[test]
fn a_click_action_activates_the_element() -> TestResult {
    let _rt = Runtime::new().enter();
    let count = signal(0);
    let enabled = signal(true);
    let mut ui = TestUi::new(move || {
        button("Add", move || count.update(|count| *count += 1)).enabled(enabled)
    });
    assert!(
        ui.access_node(Role::Button, "Add")?
            .actions
            .contains(&Action::Click)
    );
    assert!(ui.perform(Role::Button, "Add", Action::Click)?);
    assert_eq!(count.get_untracked(), 1);
    enabled.set(false);
    assert!(ui.access_node(Role::Button, "Add")?.disabled);
    assert!(!ui.perform(Role::Button, "Add", Action::Click)?);
    assert_eq!(count.get_untracked(), 1);
    Ok(())
}

#[test]
fn a_focus_action_moves_focus() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div()
            .child(button("First", || {}))
            .child(button("Second", || {}))
    });
    assert!(!ui.access_node(Role::Button, "Second")?.focused);
    assert!(ui.perform(Role::Button, "Second", Action::Focus)?);
    assert!(ui.access_node(Role::Button, "Second")?.focused);
    assert!(!ui.access_node(Role::Button, "First")?.focused);
    ui.press("tab");
    assert!(ui.access_node(Role::Button, "First")?.focused);
    Ok(())
}

#[test]
fn state_and_labels_follow_signals() -> TestResult {
    let _rt = Runtime::new().enter();
    let checked = signal(false);
    let name = signal("Off".to_owned());
    let open = signal(false);
    let mut ui = TestUi::new(move || {
        view! { <div role=Role::CheckBox access_label=name is_checked=checked expanded=open
        on_activate={move || checked.toggle()}/> }
    });
    let node = ui.access_node(Role::CheckBox, "Off")?;
    assert_eq!(node.toggled, Some(Toggled::False));
    assert_eq!(node.expanded, Some(false));
    ui.perform(Role::CheckBox, "Off", Action::Click)?;
    name.set("On".to_owned());
    open.set(true);
    let node = ui.access_node(Role::CheckBox, "On")?;
    assert_eq!(node.toggled, Some(Toggled::True));
    assert_eq!(node.expanded, Some(true));
    Ok(())
}

#[test]
fn one_change_updates_one_node() -> TestResult {
    let _rt = Runtime::new().enter();
    let checked = signal(false);
    let mut ui = TestUi::new(move || {
        view! {
            <div>
                {..(0..50).map(|i| button(&format!("Row {i}"), || {}))}
                <div role=Role::Switch access_label="Wifi" is_checked=checked/>
            </div>
        }
    });
    assert_eq!(ui.access_nodes().len(), 101);
    let before = ui.stats();
    checked.set(true);
    assert_eq!(
        ui.access_node(Role::Switch, "Wifi")?.toggled,
        Some(Toggled::True)
    );
    assert_eq!((ui.stats() - before).access_nodes, 1);
    Ok(())
}

#[test]
fn mounting_and_hiding_change_the_tree() -> TestResult {
    let _rt = Runtime::new().enter();
    let shown = signal(false);
    let private = signal(false);
    let mut ui = TestUi::new(move || {
        div()
            .child(button("Always", || {}))
            .child(show(move || shown.get(), || button("Sometimes", || {})))
            .child(div().access_hidden(private).child(button("Maybe", || {})))
            .child(div().invisible().child(button("Never", || {})))
    });
    assert_eq!(names(&mut ui), ["Always", "Maybe"]);
    shown.set(true);
    private.set(true);
    assert_eq!(names(&mut ui), ["Always", "Sometimes"]);
    shown.set(false);
    private.set(false);
    assert_eq!(names(&mut ui), ["Always", "Maybe"]);
    Ok(())
}

#[test]
fn bounds_follow_layout_transforms_and_scrolling() -> TestResult {
    let _rt = Runtime::new().enter();
    let shift = signal(0.0);
    let state = signal(ScrollState::default());
    let mut ui = TestUiBuilder::default().size(100.0, 100.0).build(move || {
        scroll()
            .scroll_state(state)
            .size_full()
            .child(div().h(40.0).translate_x(shift).child(tile("Moved")))
            .children((0..10).map(|index| tile(&format!("Tile {index}"))))
    });
    assert_eq!(bounds(&mut ui, "Moved")?, Rect::new(0.0, 0.0, 20.0, 40.0));
    assert_eq!(bounds(&mut ui, "Tile 1")?, Rect::new(0.0, 80.0, 20.0, 40.0));
    shift.set(30.0);
    assert_eq!(bounds(&mut ui, "Moved")?, Rect::new(30.0, 0.0, 20.0, 40.0));
    let before = ui.stats();
    ui.scroll_on("Tile 0", Point::new(0.0, -50.0))?;
    ui.run(0.5);
    assert_eq!(bounds(&mut ui, "Tile 1")?, Rect::new(0.0, 30.0, 20.0, 40.0));
    assert_eq!((ui.stats() - before).access_nodes, 2);
    assert!(
        bounds(&mut ui, "Tile 9").is_err(),
        "clipped rows are left out"
    );
    assert!(ui.perform(Role::Button, "Tile 3", Action::ScrollIntoView)?);
    ui.run(0.5);
    assert_eq!(bounds(&mut ui, "Tile 3")?, Rect::new(0.0, 60.0, 20.0, 40.0));
    assert_eq!(state.get_untracked().offset, Point::new(0.0, 100.0));
    Ok(())
}

#[test]
fn the_tree_is_in_device_pixels_and_follows_the_scale() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default()
        .scale(2.0)
        .build(|| div().child(tile("Scaled")));
    let logical = Rect::new(0.0, 0.0, 20.0, 40.0);
    assert_eq!(bounds(&mut ui, "Scaled")?, logical);
    let tree = ui.accessibility_tree();
    let root = tree.nodes.first().ok_or("an empty tree")?;
    assert_eq!(root.1.transform().map(|t| t.as_coeffs()[0]), Some(2.0));
    ui.set_device_scale(3.0);
    ui.advance(0.0);
    let update = ui
        .accessibility_update()
        .ok_or("no update for a new scale")?;
    let root = update.nodes.last().ok_or("an empty update")?;
    assert_eq!(root.1.transform().map(|t| t.as_coeffs()[0]), Some(3.0));
    Ok(())
}

#[test]
fn increment_and_decrement_reach_typed_handlers() -> TestResult {
    let _rt = Runtime::new().enter();
    let level = signal(5.0);
    let mut ui = TestUi::new(move || {
        view! {
            <div role=Role::Slider access_label="Volume" live=Live::Polite numeric_value=level
                 value_step=1.0 on_action={move |_: &Increment| level.update(|n| *n += 1.0)}
                 on_action={move |_: &Decrement| level.update(|n| *n -= 1.0)}/>
        }
        .value_range(0.0, 10.0)
    });
    assert_eq!(
        ui.access_node(Role::Slider, "Volume")?.numeric_value,
        Some(5.0)
    );
    assert!(ui.perform(Role::Slider, "Volume", Action::Increment)?);
    assert!(ui.perform(Role::Slider, "Volume", Action::Increment)?);
    assert!(ui.perform(Role::Slider, "Volume", Action::Decrement)?);
    assert_eq!(
        ui.access_node(Role::Slider, "Volume")?.numeric_value,
        Some(6.0)
    );
    Ok(())
}

#[test]
fn a_text_field_reports_and_accepts_its_value() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal("hello".to_owned());
    let mut ui = TestUi::new(move || div().child(editable(value).access_label("Name")));
    let node = ui.access_node(Role::TextInput, "Name")?;
    assert_eq!(node.value.as_deref(), Some("hello"));
    for (action, input, expected) in [
        (Action::SetValue, "world", "world"),
        (Action::ReplaceSelectedText, "!", "world!"),
    ] {
        let data = ActionData::Value(input.into());
        assert!(ui.perform_with(Role::TextInput, "Name", action, data)?);
        assert_eq!(value.get_untracked(), expected);
        let node = ui.access_node(Role::TextInput, "Name")?;
        assert_eq!(node.value.as_deref(), Some(expected));
    }
    Ok(())
}

#[test]
fn activatable_elements_are_buttons_named_by_their_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let title = signal("Open".to_owned());
    let mixed = signal(Toggled::Mixed);
    let mut ui = TestUi::new(move || {
        view! {
            <div>
                <div on_activate={|| {}}>
                    <text>{title}</text><div><text>"file"</text></div>
                    <div on_activate={|| {}}><text>"Close"</text></div>
                </div>
                <div role=Role::CheckBox access_label="All" toggled=mixed/>
            </div>
        }
    });
    ui.access_node(Role::Button, "Open file")?;
    ui.access_node(Role::Button, "Close")?;
    title.set("Save".to_owned());
    ui.access_node(Role::Button, "Save file")?;
    assert_eq!(
        ui.access_node(Role::CheckBox, "All")?.toggled,
        Some(Toggled::Mixed)
    );
    mixed.set(Toggled::True);
    assert_eq!(
        ui.access_node(Role::CheckBox, "All")?.toggled,
        Some(Toggled::True)
    );
    Ok(())
}

#[test]
fn reactivation_starts_with_current_explicit_properties() -> TestResult {
    let _rt = Runtime::new().enter();
    let title = signal("Save".to_owned());
    let mut ui = TestUi::new(move || button("Inferred", || {}).access_label(title));
    assert!(ui.accessibility_update().is_none());
    ui.access_node(Role::Button, "Save")?;
    ui.stop_accessibility();
    title.set("Apply".to_owned());
    ui.advance(0.0);
    assert!(ui.accessibility_update().is_none());
    let initial = ui.accessibility_tree();
    assert!(initial.tree.is_some());
    assert!(
        initial
            .nodes
            .iter()
            .any(|(_, n)| n.role() == Role::Button && n.label() == Some("Apply"))
    );
    assert!(ui.accessibility_update().is_none());
    Ok(())
}

#[test]
fn a_missing_node_is_an_error() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| button("Here", || {}));
    let missing = ui.access_node(Role::Button, "Gone");
    assert!(matches!(missing, Err(TestError::NoAccessNode { .. })));
}

fn button(name: &str, on_activate: impl Fn() + 'static) -> Div {
    div()
        .role(Role::Button)
        .focusable()
        .on_activate(on_activate)
        .child(text(name))
}

fn tile(name: &str) -> Div {
    view! { <div role=Role::Button access_label=name test_id=name w=20.0 h=40.0 flex_none/> }
}

fn outline(nodes: &[AccessNode]) -> Vec<(Role, &str)> {
    nodes
        .iter()
        .map(|node| (node.role, node.name.as_str()))
        .collect()
}

fn names(ui: &mut TestUi) -> Vec<String> {
    let nodes = ui.access_nodes().into_iter();
    nodes
        .filter(|node| node.role == Role::Button)
        .map(|node| node.name)
        .collect()
}

fn bounds(ui: &mut TestUi, name: &str) -> Result<Rect, TestError> {
    Ok(ui.access_node(Role::Button, name)?.bounds)
}
