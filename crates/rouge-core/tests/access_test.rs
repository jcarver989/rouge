use accesskit::{ActionRequest, NodeId, TextPosition, TextSelection, TreeId};
use accesskit_consumer::NodeRef;
use rouge_core::access::{Action, ActionData, Decrement, Increment, Live, Role, Toggled};
use rouge_core::testing::{AccessNode, TestError, TestResult, TestUi, TestUiBuilder};
use rouge_core::{
    Div, EditAction, ImeEvent, Point, Rect, ScrollState, Scrollable, Styled, div, editable, scroll,
    show, text, view,
};
use rouge_reactive::{Runtime, signal};
use std::error::Error;

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

#[test]
fn the_consumer_reads_and_replaces_a_backward_unicode_selection() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal("a e\u{301} 👩‍💻 אבג".to_owned());
    let mut ui = TestUi::new(move || editable(value).w(320.).access_label("Name").autofocus());
    let field = editor(&mut ui)?;
    assert_eq!(field.document_range().text(), value.get());
    let id = field.locate().0;
    let at = |index| field.text_position_from_global_utf16_index(index);
    let (start, end) = (
        at(2).ok_or("start")?.to_raw(),
        at(10).ok_or("end")?.to_raw(),
    );
    assert!(select(&mut ui, id, start, end));
    assert!(select(&mut ui, id, end, start));
    let field = editor(&mut ui)?;
    let ends = [field.text_selection_anchor(), field.text_selection_focus()]
        .map(|position| position.map(|position| position.to_global_utf16_index()));
    assert_eq!(ends, [Some(10), Some(2)]);
    assert_eq!(selected(&mut ui)?, "e\u{301} 👩‍💻");
    assert!(ui.perform_with(
        Role::TextInput,
        "Name",
        Action::ReplaceSelectedText,
        ActionData::Value("X".into())
    )?);
    assert_eq!(value.get(), "a X אבג");
    ui.dispatch_action(EditAction::Undo);
    assert_eq!(selected(&mut ui)?, "e\u{301} 👩‍💻");
    Ok(())
}

#[test]
fn moving_the_caret_only_publishes_the_editor_and_blink_publishes_nothing() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| editable(signal("one two".into())).w(200.).autofocus());
    let old = editor(&mut ui)?.document_start().to_raw();
    let before = ui.stats();
    ui.press("shift-right");
    let field = editor(&mut ui)?;
    assert_eq!(field.text_selection().ok_or("selection")?.text(), "o");
    assert_eq!(field.document_start().to_raw(), old);
    let work = ui.stats() - before;
    assert_eq!((work.access_nodes, work.text.shapes), (1, 0));
    ui.press("right");
    editor(&mut ui)?;
    let before = ui.stats();
    ui.advance(0.51);
    ui.access_tree();
    assert_eq!((ui.stats() - before).access_nodes, 0);
    Ok(())
}

#[test]
fn multiline_ranges_cross_wraps_newlines_and_the_trailing_empty_line() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal("one two three four five six seven\nאבג abc\n".to_owned());
    let mut ui = TestUi::new(move || editable(value).w(60.).multiline().autofocus());
    assert!(editor(&mut ui)?.document_end().to_line_index() >= 3);
    let (id, start, end) = whole(&mut ui)?;
    assert!(select(&mut ui, id, start, end));
    let field = editor(&mut ui)?;
    let range = field.text_selection().ok_or("selection")?;
    assert_eq!(range.text(), value.get());
    assert!(range.bounding_boxes().len() >= 3);
    assert_eq!(range.end().to_raw(), end);
    ui.type_text("done");
    assert_eq!(value.get(), "done");
    Ok(())
}

#[test]
fn empty_preedit_commit_and_cancel_publish_the_real_buffer_not_the_hint() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        editable(signal(String::new()))
            .placeholder("Hint", 0xffffffu32)
            .autofocus()
    });
    let field = editor(&mut ui)?;
    assert_eq!(field.document_range().text(), "");
    let caret = field.text_selection_focus().ok_or("empty caret")?;
    assert_eq!(caret.to_raw(), field.document_start().to_raw());
    for text in ["日", ""] {
        ui.ime(ImeEvent::Preedit {
            text: text.into(),
            cursor: None,
        });
        assert_eq!(editor(&mut ui)?.document_range().text(), text);
    }
    ui.ime(ImeEvent::Commit("本".into()));
    let field = editor(&mut ui)?;
    assert_eq!(field.document_range().text(), "本");
    let caret = field.text_selection_focus();
    assert_eq!(caret.map(|focus| focus.to_global_utf16_index()), Some(1));
    Ok(())
}

#[test]
fn invalid_or_retired_positions_cannot_change_a_live_editor() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal("first".to_owned());
    let live = signal(true);
    let mut ui = TestUi::new(move || {
        div()
            .child(show(
                move || live.get(),
                move || editable(value).autofocus(),
            ))
            .child(editable(signal("other".into())))
    });
    let (id, start, end) = whole(&mut ui)?;
    let mut invalid = end;
    invalid.character_index = usize::MAX;
    assert!(!select(&mut ui, id, start, invalid));
    ui.press("tab");
    let other = editor(&mut ui)?.document_start().to_raw();
    assert_ne!(other.node, start.node);
    assert!(!select(&mut ui, id, start, other));
    value.set("changed".into());
    assert!(!select(&mut ui, id, start, end));
    ui.advance(0.0);
    ui.access_tree();
    assert!(!select(&mut ui, id, start, end));
    for mounted in [false, true] {
        live.set(mounted);
        ui.advance(0.0);
        ui.access_tree();
    }
    assert!(!select(&mut ui, id, start, end));
    assert_eq!(value.get(), "changed");
    Ok(())
}

#[test]
fn hidden_disabled_and_deactivated_editors_reject_selection_requests() -> TestResult {
    let _rt = Runtime::new().enter();
    let hidden = signal(false);
    let enabled = signal(true);
    let value = signal("first".to_owned());
    let mut ui = TestUi::new(move || {
        div()
            .access_hidden(hidden)
            .enabled(enabled)
            .child(editable(value).autofocus().access_label("Field"))
    });
    let (id, start, end) = whole(&mut ui)?;
    enabled.set(false);
    assert!(!select(&mut ui, id, start, end));
    enabled.set(true);
    hidden.set(true);
    assert!(!select(&mut ui, id, start, end));
    ui.access_tree();
    hidden.set(false);
    assert!(ui.perform(Role::TextInput, "Field", Action::Focus)?);
    assert!(!select(&mut ui, id, start, end));
    let (_, start, end) = whole(&mut ui)?;
    ui.stop_accessibility();
    assert!(!select(&mut ui, id, start, end));
    value.set("next".into());
    assert_eq!(editor(&mut ui)?.document_range().text(), "next");
    let (_, from, to) = whole(&mut ui)?;
    assert!(!select(&mut ui, id, start, end));
    assert!(select(&mut ui, id, from, to));
    Ok(())
}

#[test]
fn long_lines_keep_word_boundaries_across_run_splits() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal(format!("{} abc def", "x".repeat(255)));
    let mut ui = TestUi::new(move || editable(value).autofocus());
    let field = editor(&mut ui)?;
    let first = field.document_start();
    let word = first.forward_to_word_start();
    assert_eq!(word.to_global_utf16_index(), 256);
    assert_ne!(first.to_raw().node, word.to_raw().node);
    assert_eq!(word.forward_to_word_start().to_global_utf16_index(), 260);
    let (id, start, end) = whole(&mut ui)?;
    assert!(select(&mut ui, id, start, end));
    assert_eq!(selected(&mut ui)?, value.get());
    Ok(())
}

#[test]
fn an_unrepresentable_cluster_exposes_the_value_without_corrupt_ranges() -> TestResult {
    let _rt = Runtime::new().enter();
    let value = signal(String::from("ok"));
    let mut ui = TestUi::new(move || editable(value).autofocus().access_label("Field"));
    let (id, start, end) = whole(&mut ui)?;
    value.set(format!("a{}", "\u{301}".repeat(180)));
    let node = ui.access_node(Role::TextInput, "Field")?;
    assert_eq!(node.value, Some(value.get()));
    assert!(!node.actions.contains(&Action::SetTextSelection));
    let field = ui.access_tree().state().focus().ok_or("field")?;
    assert!(!field.supports_text_ranges());
    assert!(!select(&mut ui, id, start, end));
    value.set("ok again".into());
    assert_eq!(editor(&mut ui)?.document_range().text(), "ok again");
    Ok(())
}

#[test]
fn range_geometry_tracks_editor_scroll_reflow_and_parent_translation() -> TestResult {
    let _rt = Runtime::new().enter();
    let width = signal(85.0);
    let shift = signal(15.0);
    let value = signal("one two three four five six seven eight nine ten".to_owned());
    let mut ui = TestUi::new(move || {
        div()
            .translate_x(shift)
            .child(editable(value).multiline().max_rows(2).w(width).autofocus())
    });
    let (id, _, end) = whole(&mut ui)?;
    assert!(select(&mut ui, id, end, end));
    for (w, x) in [(85., 15.), (60., 35.)] {
        width.set(w);
        shift.set(x);
        let field = editor(&mut ui)?;
        let focus = field.text_selection_focus().ok_or("focus")?;
        assert_eq!(focus.to_global_utf16_index(), value.get().len());
        let boxes = field.text_selection().ok_or("selection")?.bounding_boxes();
        let bounds = boxes.first().ok_or("caret bounds")?;
        let caret = ui.text_cursor_area().ok_or("caret")?;
        assert!(
            (bounds.x0 - f64::from(caret.origin.x)).abs() <= 1.0,
            "{bounds:?} {caret:?}"
        );
        assert!(
            (bounds.y0 - f64::from(caret.origin.y)).abs() <= 0.01
                && (bounds.y1 - f64::from(caret.max().y)).abs() <= 0.01,
            "{bounds:?} {caret:?}"
        );
    }
    Ok(())
}

fn editor(ui: &mut TestUi) -> Result<NodeRef<'_>, Box<dyn Error>> {
    ui.advance(0.0);
    let field = ui.access_tree().state().focus().ok_or("no focus")?;
    assert!(field.supports_text_ranges());
    Ok(field)
}

fn whole(ui: &mut TestUi) -> Result<(NodeId, TextPosition, TextPosition), Box<dyn Error>> {
    let field = editor(ui)?;
    let (start, end) = (field.document_start(), field.document_end());
    Ok((field.locate().0, start.to_raw(), end.to_raw()))
}

fn selected(ui: &mut TestUi) -> Result<String, Box<dyn Error>> {
    Ok(editor(ui)?.text_selection().ok_or("no selection")?.text())
}

fn select(ui: &mut TestUi, target_node: NodeId, anchor: TextPosition, focus: TextPosition) -> bool {
    ui.accessibility_action(ActionRequest {
        action: Action::SetTextSelection,
        target_tree: TreeId::ROOT,
        target_node,
        data: Some(ActionData::SetTextSelection(TextSelection {
            anchor,
            focus,
        })),
    })
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
