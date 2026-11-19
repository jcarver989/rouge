use rouge_core::testing::{TestError, TestResult, TestUi, assert_near};
use rouge_core::{Color, Length, Point, Styled, UiPalette, View, div, pct, px, text, view};
use rouge_reactive::{Runtime, Signal, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::*;

use SearchPickerEvent::{Chose, Highlighted, Query};

#[test]
fn an_item_matches_its_prefix_label_subtitle_details_and_keywords() {
    let _entered = Runtime::new().enter();
    let (mut ui, handle, _) = SearchPickerTest::new(vec![
        PickerItem::new("match", "Label")
            .prefix("pfx")
            .subtitle("Sub")
            .detail("first")
            .keywords("hidden"),
        PickerItem::new("other", "Other"),
    ])
    .build();
    for query in ["pfx", "Label", "Sub", "first", "hidden"] {
        handle.query().set(query.to_owned());
        assert_eq!(visible(&mut ui, ["match", "other"]), ["match"], "{query}");
    }
}

#[test]
fn matches_are_ranked_best_first_and_misses_dropped() {
    let _entered = Runtime::new().enter();
    let (mut ui, handle, events) = SearchPickerTest::new(vec![
        PickerItem::new("toggle", "Toggle split / unified"),
        PickerItem::new("submit", "Submit review"),
        PickerItem::new("stage", "Stage all"),
    ])
    .build();
    let ids = ["toggle", "submit", "stage"];
    handle.query().set("sub".to_owned());
    assert_eq!(visible(&mut ui, ids), ["submit"]);
    handle.query().set("st".to_owned());
    assert_eq!(visible(&mut ui, ids)[0], "stage");
    handle.query().set("zzz".to_owned());
    assert_eq!(visible(&mut ui, ids), Vec::<&str>::new());
    assert!(ui.text_bounds("No matching items").is_some());
    handle.choose();
    assert!(!events.with(|events| events.iter().any(|event| matches!(event, Chose(_)))));
}

#[test]
fn groups_stay_together_in_first_appearance_order_under_their_headers() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, _, _) = SearchPickerTest::new(vec![
        PickerItem::new("next", "next").group("Navigation"),
        PickerItem::new("theme", "theme").group("View"),
        PickerItem::new("previous", "previous").group("Navigation"),
    ])
    .build();
    assert_eq!(
        visible(&mut ui, ["next", "theme", "previous"]),
        ["next", "previous", "theme"]
    );
    let navigation = ui.text_bounds("Navigation").ok_or("no Navigation header")?;
    let view = ui.text_bounds("View").ok_or("no View header")?;
    assert!(navigation.origin.y < ui.rect_of("next")?.origin.y);
    assert!(ui.rect_of("previous")?.origin.y < view.origin.y);
    assert!(view.origin.y < ui.rect_of("theme")?.origin.y);
    Ok(())
}

#[test]
fn the_highlight_skips_disabled_items_and_stops_at_either_end() {
    let _entered = Runtime::new().enter();
    let (_ui, handle, events) = SearchPickerTest::new(vec![
        PickerItem::new("off", "off").disabled(true),
        PickerItem::new("first", "first"),
        PickerItem::new("off-again", "off").disabled(true),
        PickerItem::new("last", "last"),
    ])
    .build();
    handle.move_highlight(1);
    handle.move_highlight(8);
    handle.move_highlight(-8);
    handle.highlight(2);
    handle.highlight(3);
    handle.choose();
    assert_eq!(
        events.get(),
        [Highlighted(3), Highlighted(1), Highlighted(3), Chose(3)]
    );
}

#[test]
fn an_unfiltered_picker_reports_the_query_and_lists_everything() {
    let _entered = Runtime::new().enter();
    let (mut ui, handle, events) = SearchPickerTest::new(vec![
        PickerItem::new("alpha", "Alpha"),
        PickerItem::new("beta", "Beta"),
    ])
    .unfiltered()
    .build();
    handle.query().set("zzz".to_owned());
    assert_eq!(visible(&mut ui, ["alpha", "beta"]), ["alpha", "beta"]);
    assert_eq!(events.get(), [Query("zzz".to_owned())]);
}

#[test]
fn a_limited_picker_limits_its_matches_not_its_items() {
    let _entered = Runtime::new().enter();
    let (mut ui, handle, _) = SearchPickerTest::new(vec![
        PickerItem::new("submit", "Submit review"),
        PickerItem::new("stage", "Stage all"),
        PickerItem::new("show", "Show split"),
    ])
    .max_matches(1)
    .build();
    let ids = ["submit", "stage", "show"];
    assert_eq!(visible(&mut ui, ids), ["submit"]);
    handle.query().set("split".to_owned());
    assert_eq!(visible(&mut ui, ids), ["show"]);
}

#[test]
fn only_matching_items_can_be_highlighted() {
    let _entered = Runtime::new().enter();
    let (mut ui, handle, events) = SearchPickerTest::new(vec![
        PickerItem::new("submit", "Submit review"),
        PickerItem::new("stage", "Stage all"),
        PickerItem::new("show", "Show split"),
    ])
    .build();
    handle.query().set("split".to_owned());
    ui.run(0.0);
    handle.highlight(0);
    handle.choose();
    assert_eq!(
        events.get(),
        [Query("split".to_owned()), Highlighted(2), Chose(2)]
    );
}

#[test]
fn replacing_items_keeps_the_query_and_regroups() {
    let _entered = Runtime::new().enter();
    let (mut ui, handle, _) = SearchPickerTest::new(vec![
        PickerItem::new("gate", "gate"),
        PickerItem::new("beta", "beta"),
    ])
    .build();
    let ids = ["gate", "beta", "delta", "gamma", "alpha"];
    handle.query().set("ga".to_owned());
    assert_eq!(visible(&mut ui, ids), ["gate"]);
    handle.set_items(vec![
        PickerItem::new("delta", "delta").group("Sooner"),
        PickerItem::new("gamma", "gamma").group("Later"),
        PickerItem::new("alpha", "alpha").group("Sooner"),
    ]);
    assert_eq!(visible(&mut ui, ids), ["gamma"]);
    assert_eq!(handle.query().get(), "ga");
    assert!(ui.text_bounds("Later").is_some() && ui.text_bounds("Sooner").is_none());
    handle.query().set(String::new());
    assert_eq!(visible(&mut ui, ids), ["delta", "alpha", "gamma"]);
}

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
            view! { <search_picker handle test_id="picker" searchable="Find" preview=preview/> },
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
        view! { <search_picker handle test_id="picker" searchable="Find" max_list_height=Length::Px(104.0)/> }
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
        let picker = view! { <picker_list items test_id="list" on_choose={move |index: usize| selected.set(Some(index))}/> };
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

#[test]
fn a_long_list_beside_a_preview_scrolls_inside_the_picker() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(1000.0, 800.0).build(|| {
        let items = (0..40).map(|i| PickerItem::new(format!("option-{i}"), format!("Item {i}"))).collect();
        let handle = SearchPickerHandle::new(items, |_| {});
        let preview = |index: usize| text(format!("Preview {index}")).into_any();
        view! {
            <search_picker handle test_id="picker" searchable="Find" w=px(tokens::PICKER_PREVIEW_WIDTH)
                           h=px(tokens::PICKER_PREVIEW_HEIGHT) max_list_height=pct(100.0) preview=preview/>
        }
    });
    let picker = ui.rect_of("picker")?;
    assert!(ui.rect_of("picker-results")?.max().y <= picker.max().y);
    ui.click_on("picker-query")?;
    for _ in 0..6 {
        ui.press("pagedown");
    }
    ui.run(0.15);
    let last = ui.rect_of("option-39")?;
    assert!(
        last.origin.y >= picker.origin.y && last.max().y <= picker.max().y,
        "{last:?} outside {picker:?}"
    );
    Ok(())
}

#[test]
fn a_footer_of_key_hints_sets_each_key_in_a_kbd_beside_its_action_on_the_unit() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(600.0, 400.0).build(|| {
        let handle = SearchPickerHandle::new(vec![PickerItem::new("only", "Only")], |_| {});
        let hints = key_hints([("↑↓", "Move"), ("↵", "Open"), ("Esc", "Close")]).test_id("hints");
        view! { <search_picker handle test_id="picker" footer=hints/> }
    });
    let hints = ui.rect_of("hints")?;
    let picker = ui.rect_of("picker")?;
    assert!(
        hints.max().y <= picker.max().y,
        "the hints sit inside the picker: {hints:?} {picker:?}"
    );
    assert_eq!(hints.size.height % tokens::UNIT, 0.0, "{hints:?}");
    let (keys, action) = (
        ui.text_bounds("Esc").ok_or("Esc")?,
        ui.text_bounds("Close").ok_or("Close")?,
    );
    let (move_keys, open) = (
        ui.text_bounds("↑↓").ok_or("↑↓")?,
        ui.text_bounds("Open").ok_or("Open")?,
    );
    assert!(
        move_keys.max().x < open.origin.x
            && open.max().x < keys.origin.x
            && keys.max().x < action.origin.x
    );
    Ok(())
}

#[test]
fn one_highlight_layer_slides_between_rows_and_scrolls_with_them() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, handle) = component_test().size(600.0, 460.0).build_with(|| {
        let items = (0..40)
            .map(|i| PickerItem::new(format!("option-{i}"), format!("Item {i}")))
            .collect();
        let handle = SearchPickerHandle::new(items, |_| {});
        let picker =
            view! { <search_picker handle test_id="picker" max_list_height=Length::Px(112.0)/> };
        (picker, handle)
    });
    ui.run(0.5);
    assert!(covers(&mut ui, "option-0")?);
    let accent = UiPalette::light().map(Color::from).accent.to_rgba8();
    let row = ui.rect_of("option-0")?;
    assert_near(
        ui.pixel(row.origin.x as u32 + 2, row.center().y as u32)?,
        accent,
        "the highlight is a flat accent fill",
    );
    let created = ui.stats().elements_created;
    handle.move_highlight(2);
    ui.advance(0.0);
    assert!(ui.is_animating(), "the highlight slides");
    ui.run(0.5);
    assert!(covers(&mut ui, "option-2")?);
    assert_eq!(ui.stats().elements_created, created, "the same layer moved");
    let before = ui.rect_of("option-2")?;
    ui.scroll_on("picker-results", Point::new(0.0, -20.0))?;
    ui.advance(1.0 / 60.0);
    assert_ne!(ui.rect_of("option-2")?, before, "the list scrolled");
    assert!(
        covers(&mut ui, "option-2")?,
        "scrolling carries the highlight with its row"
    );
    ui.run(0.5);
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

struct SearchPickerTest {
    items: Vec<PickerItem>,
    unfiltered: bool,
    max_matches: Option<usize>,
}

impl SearchPickerTest {
    fn new(items: Vec<PickerItem>) -> Self {
        Self {
            items,
            unfiltered: false,
            max_matches: None,
        }
    }

    fn unfiltered(mut self) -> Self {
        self.unfiltered = true;
        self
    }

    fn max_matches(mut self, limit: usize) -> Self {
        self.max_matches = Some(limit);
        self
    }

    fn build(self) -> (TestUi, SearchPickerHandle, Signal<Vec<SearchPickerEvent>>) {
        let (ui, (handle, events)) = component_test().size(600.0, 400.0).build_with(|| {
            let events = signal(Vec::new());
            let mut handle = SearchPickerHandle::new(self.items, move |event| {
                events.update(|events| events.push(event));
            });
            if self.unfiltered {
                handle = handle.unfiltered();
            }
            if let Some(limit) = self.max_matches {
                handle = handle.max_matches(limit);
            }
            (
                view! { <search_picker handle test_id="picker"/> },
                (handle, events),
            )
        });
        (ui, handle, events)
    }
}

fn covers(ui: &mut TestUi, row: &str) -> Result<bool, TestError> {
    let (highlight, row) = (ui.rect_of("picker-highlight")?, ui.rect_of(row)?);
    let near = |a: f32, b: f32| (a - b).abs() < 0.1;
    Ok(near(highlight.origin.x, row.origin.x)
        && near(highlight.origin.y, row.origin.y)
        && near(highlight.size.width, row.size.width)
        && near(highlight.size.height, row.size.height))
}

fn visible<const N: usize>(ui: &mut TestUi, ids: [&'static str; N]) -> Vec<&'static str> {
    ui.run(0.0);
    let mut shown: Vec<_> = ids
        .into_iter()
        .filter_map(|id| ui.bounds_of(id).map(|rect| (rect.origin.y, id)))
        .collect();
    shown.sort_by(|a, b| a.0.total_cmp(&b.0));
    shown.into_iter().map(|(_, id)| id).collect()
}
