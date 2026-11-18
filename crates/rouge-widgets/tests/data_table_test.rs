use rouge_core::testing::{TestResult, TestUi, assert_near};
use rouge_core::view;
use rouge_core::{Color, Point, Styled, UiPalette, div, text};
use rouge_reactive::{KeyedList, Runtime, Signal, keyed_list, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{Sort, column, data_table, table_rows};

#[test]
fn rows_select_by_pointer_and_keyboard_and_open_on_enter() -> TestResult {
    let _entered = Runtime::new().enter();
    let Tracks {
        mut ui,
        selection,
        opened,
        ..
    } = tracks();
    assert!(ui.text_bounds("Title").is_some() && ui.text_bounds("Glasswork").is_some());
    ui.click_on("tracks-row-1")?;
    assert_eq!(selection.get(), Some("Harmattan"));
    ui.run(0.3);
    let row = ui.rect_of("tracks-row-1")?;
    let accent = UiPalette::light().map(Color::from).accent.to_rgba8();
    assert_near(
        ui.pixel(row.origin.x as u32 + 2, row.center().y as u32)?,
        accent,
        "the selected row is a flat accent fill",
    );
    ui.press("down");
    assert_eq!(selection.get(), Some("Low Tide"));
    ui.press("down");
    assert_eq!(
        selection.get(),
        Some("Low Tide"),
        "it stops at the last row"
    );
    ui.press("home");
    ui.press("enter");
    assert_eq!(opened.get(), Some("Glasswork"));
    ui.double_click_on("tracks-row-2")?;
    assert_eq!(opened.get(), Some("Low Tide"));
    Ok(())
}

#[test]
fn a_sortable_header_writes_the_sort_signal_and_a_keyed_list_reorders_in_place() -> TestResult {
    let _entered = Runtime::new().enter();
    let Tracks {
        mut ui, sort, list, ..
    } = tracks();
    let sorted = |descending| {
        Some(Sort {
            column: 0,
            descending,
        })
    };
    ui.click_on("tracks-column-0")?;
    assert_eq!(sort.get(), sorted(false));
    ui.click_on("tracks-column-0")?;
    assert_eq!(sort.get(), sorted(true));
    ui.click_on("tracks-column-1")?;
    assert_eq!(sort.get(), sorted(true), "only sortable columns sort");
    let first = ui.rect_of("tracks-row-0")?;
    let top = ui
        .text_bounds("Glasswork")
        .ok_or("the first row is drawn")?;
    assert!(first.contains(top.center()));
    list.move_to(&"Low Tide", 0);
    ui.run(0.3);
    let top = ui.text_bounds("Low Tide").ok_or("the moved row is drawn")?;
    assert!(first.contains(top.center()), "rows follow the list's order");
    Ok(())
}

#[test]
fn a_column_width_is_a_signal_that_its_grip_drags() -> TestResult {
    let _entered = Runtime::new().enter();
    let width = signal(120.0f32);
    let mut ui = component_test().size(400.0, 200.0).build(move || {
        let rows = table_rows(|| 3, |index| index);
        let table = view! {
            <data_table rows=rows test_id="grid"
                 column=column("Index", |index: &usize| view! { <text>{index.to_string()}</text> }).width(width)
                 column={column("Double", |index: &usize| {
                     view! { <text>{(index * 2).to_string()}</text> }
                 })}/>
        };
        view! { <div p=10.0 h=180.0 flex col>{table}</div> }
    });
    let second = ui
        .text_bounds("Double")
        .ok_or("the header is drawn")?
        .origin
        .x;
    let grip = ui.center_of("grid-resize-0")?;
    ui.drag(grip, grip + Point::new(40.0, 0.0));
    assert_eq!(width.get(), 160.0);
    ui.run(0.2);
    let moved = ui
        .text_bounds("Double")
        .ok_or("the header is drawn")?
        .origin
        .x;
    assert_eq!(
        moved,
        second + 40.0,
        "header and cells share the column width"
    );
    ui.drag(grip + Point::new(40.0, 0.0), grip - Point::new(400.0, 0.0));
    assert_eq!(width.get(), 40.0, "a column keeps a minimum width");
    Ok(())
}

#[test]
fn a_hundred_thousand_rows_mount_only_what_is_visible_while_scrolling() -> TestResult {
    let _entered = Runtime::new().enter();
    let selection = signal(None::<usize>);
    let mut ui = component_test().size(600.0, 400.0).build(move || {
        let rows = table_rows(|| 100_000, |index| index);
        let table = view! {
            <data_table rows=rows test_id="grid" selection=selection
                 column=column("Index", |index: &usize| view! { <text>{format!("Row {index}")}</text> }).flex(1.4)
                 column={column("Square", |index: &usize| {
                     view! { <text>{(index * index).to_string()}</text> }
                 })}
                 column={column("Parity", |index: &usize| {
                     view! {
                         <text>
                             {if index.is_multiple_of(2) {
                                                 "even"
                                             } else {
                                                 "odd"
                                             }}
                         </text>
                     }
                 })}/>
        };
        view! { <div size_full flex col>{table}</div> }
    });
    let mounted = ui.stats().elements;
    assert!(mounted < 400, "{mounted} elements for the first screen");
    let header = ui.text_bounds("Square").ok_or("the header is drawn")?;
    for _ in 0..200 {
        ui.pointer_moved(Point::new(300.0, 200.0));
        ui.scroll(Point::new(0.0, -400.0));
        ui.advance(1.0 / 60.0);
    }
    ui.run(1.0);
    assert!(
        ui.bounds_of("grid-row-0").is_none(),
        "the first rows were recycled"
    );
    let after = ui.stats().elements;
    assert!(
        after < mounted * 2,
        "the mounted set stays bounded: {mounted} then {after}"
    );
    assert_eq!(
        ui.text_bounds("Square").map(|at| at.origin),
        Some(header.origin),
        "the header stays put"
    );
    ui.press("tab");
    ui.press("end");
    ui.run(0.5);
    assert_eq!(selection.get(), Some(99_999));
    assert!(
        ui.text_bounds("Row 99999").is_some(),
        "the keyboard reveals the selected row"
    );
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

#[derive(Clone)]
struct Track {
    title: &'static str,
    plays: u32,
}

struct Tracks {
    ui: TestUi,
    selection: Signal<Option<&'static str>>,
    opened: Signal<Option<&'static str>>,
    sort: Signal<Option<Sort>>,
    list: KeyedList<&'static str, Track>,
}

fn tracks() -> Tracks {
    let (selection, opened, sort) = (signal(None), signal(None), signal(None));
    let list = keyed_list(|track: &Track| track.title);
    list.extend(library());
    let ui = component_test()
        .size(400.0, 200.0)
        .clear(Color::WHITE)
        .build(move || {
            let table = view! {
                <data_table rows=list test_id="tracks" selection=selection sort=sort
                     on_open={move |title: &'static str| opened.set(Some(title))}
                     column={column("Title", |track: &Track| view! { <text>{track.title}</text> })
                         .flex(1.4)
                         .sortable()}
                     column={column("Plays", |track: &Track| {
                         view! { <text>{track.plays.to_string()}</text> }
                     })}/>
            };
            view! { <div p=10.0 h=180.0 flex col>{table}</div> }
        });
    Tracks {
        ui,
        selection,
        opened,
        sort,
        list,
    }
}

fn library() -> [Track; 3] {
    [
        Track {
            title: "Glasswork",
            plays: 412,
        },
        Track {
            title: "Harmattan",
            plays: 96,
        },
        Track {
            title: "Low Tide",
            plays: 1280,
        },
    ]
}
