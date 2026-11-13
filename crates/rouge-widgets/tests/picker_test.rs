use rouge_core::testing::TestResult;
use rouge_core::{Length, View, div, text, view};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::*;

use SearchPickerEvent::{Chose, Highlighted};

#[test]
fn search_filters_details_and_skips_disabled_rows() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, events) = component_test().size(780.0, 360.0).build_with(|| {
        let events = signal(Vec::new());
        let handle = SearchPickerHandle::new(
            vec![
                PickerItem::new("alpha", "Alpha"),
                PickerItem::new("disabled", "Unavailable").disabled(true),
                PickerItem::new("beta", "Beta")
                    .subtitle("Nimbus")
                    .detail("revision 42"),
            ],
            move |event| events.update(|events| events.push(event)),
        );
        let preview = |index: usize| {
            div()
                .test_id(format!("preview-{index}"))
                .child(text(format!("Preview {index}")))
                .into_any()
        };
        (
            view! { <search_picker id="picker" handle searchable="Find" preview/> },
            events,
        )
    });
    ui.click_on("disabled")?;
    assert!(events.with(Vec::is_empty));
    ui.click_on("picker-query")?;
    ui.press("down");
    assert_eq!(events.get(), vec![Highlighted(2)]);
    assert!(ui.center_of("preview-2")?.x > ui.center_of("picker-results")?.x);
    ui.hover_on("alpha")?;
    assert!(ui.bounds_of("preview-0").is_some());
    ui.type_text("revision 42");
    assert!(ui.bounds_of("alpha").is_none());
    ui.press("enter");
    assert_eq!(events.with(|events| events.last().cloned()), Some(Chose(2)));
    Ok(())
}

#[test]
fn keyboard_highlight_reveals_the_entire_variable_height_row() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(600.0, 460.0).build(|| {
        let items = (0..24)
            .map(|i| {
                PickerItem::new(format!("option-{i}"), format!("Item {i}")).subtitle("Secondary text").group("Items")
            })
            .collect();
        let handle = SearchPickerHandle::new(items, |_| {});
        view! { <search_picker id="picker" handle searchable="Find" max_list_height=Length::Px(104.0)/> }
    });
    ui.press("pagedown");
    ui.press("pagedown");
    ui.run(0.15);
    let viewport = ui.rect_of("picker-results")?;
    let row = ui.rect_of("option-16")?;
    assert!(
        row.origin.y >= viewport.origin.y - 0.5 && row.max().y <= viewport.max().y + 0.5,
        "row {row:?}, viewport {viewport:?}"
    );
    assert!(row.size.height > 28.0);
    Ok(())
}

#[test]
fn grouped_list_aligns_marker_prefix_label_and_check_columns() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, selected) = component_test().size(500.0, 300.0).build_with(|| {
        let selected = signal(None);
        let items = vec![
            PickerItem::new("first", "Alpha")
                .group("Group A")
                .prefix("0123456")
                .marker(PickerMarker::Pending)
                .checked(true),
            PickerItem::new("middle", "Beta").group("Group B").prefix("1"),
            PickerItem::new("last", "Gamma").group("Group A").subtitle("Description"),
        ];
        let picker = view! { <picker_list id="list" items on_choose={move |index: usize| selected.set(Some(index))}/> };
        (picker, selected)
    });
    let first = ui.center_of("first-marker")?;
    let last = ui.center_of("last-marker")?;
    assert!((first.x - last.x).abs() < 0.1);
    let first = ui.center_of("first-label")?;
    let last = ui.center_of("last-label")?;
    assert!((first.x - last.x).abs() < 0.1);
    assert!(first.x < ui.center_of("first-check")?.x);
    assert!(first.y < last.y && last.y < ui.center_of("middle-label")?.y);
    assert!(ui.center_of("last-subtitle")?.y > last.y);
    ui.click_on("last")?;
    assert_eq!(selected.get(), Some(2));
    Ok(())
}
