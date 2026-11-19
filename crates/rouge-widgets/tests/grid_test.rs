use rouge_core::testing::{TestResult, TestUi};
use rouge_core::view;
use rouge_core::{Div, Styled, View, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::{GRID_COLUMNS, GRID_GUTTER, GRID_MARGIN};
use rouge_widgets::{column_grid, grid_cell, grid_overlay, reading_cell, reading_span};

#[test]
fn cells_span_columns_and_their_content_starts_on_a_column_edge() -> TestResult {
    let _entered = Runtime::new().enter();
    let ui = page(WIDTH, || {
        view! {
            <column_grid w=WIDTH>
                <grid_cell span=8 test_id="wide">{content("wide-content")}</grid_cell>
                <grid_cell span=4 test_id="narrow">{content("narrow-content")}</grid_cell>
                <grid_cell span=12 test_id="full">{content("full-content")}</grid_cell>
            </column_grid>
        }
    });
    let bounds = |name: &str| ui.bounds_of(name).ok_or_else(|| name.to_owned());
    assert_eq!(bounds("wide")?.size.width, 8.0 * COLUMN);
    assert_eq!(bounds("narrow")?.origin.x, 8.0 * COLUMN);
    assert_eq!(bounds("full")?.origin.y, bounds("wide")?.size.height);
    assert_eq!(bounds("wide-content")?.origin.x, GRID_MARGIN.px());
    assert_eq!(
        bounds("narrow-content")?.origin.x - bounds("wide-content")?.max().x,
        GRID_GUTTER.px(),
        "content in neighbouring cells is one gutter apart"
    );
    assert_eq!(bounds("full-content")?.size.width, WIDTH - GRID_GUTTER.px());
    Ok(())
}

#[test]
fn every_cell_takes_the_full_width_in_a_compact_window() -> TestResult {
    let _entered = Runtime::new().enter();
    let ui = page(360.0, || {
        view! {
            <column_grid w=360.0>
                <grid_cell span=8 test_id="first" h=20.0/>
                <grid_cell span=4 test_id="second" h=20.0/>
            </column_grid>
        }
    });
    let second = ui.bounds_of("second").ok_or("second")?;
    assert_eq!((second.origin.x, second.origin.y), (0.0, 20.0));
    assert_eq!(second.size.width, 360.0);
    Ok(())
}

#[test]
fn a_cell_starts_on_the_column_it_names_and_follows_a_changing_span() -> TestResult {
    let _entered = Runtime::new().enter();
    let span = signal(8u16);
    let mut ui = page(WIDTH, move || {
        view! {
            <column_grid w=WIDTH>
                <grid_cell span={move || span.get()} start={move || (GRID_COLUMNS - span.get()) / 2 + 1}
                     test_id="centred" h=20.0/>
            </column_grid>
        }
    });
    let centred = ui.bounds_of("centred").ok_or("centred")?;
    assert_eq!(
        (centred.origin.x, centred.size.width),
        (2.0 * COLUMN, 8.0 * COLUMN)
    );
    span.set(6);
    ui.advance(0.0);
    let centred = ui.bounds_of("centred").ok_or("centred")?;
    assert_eq!(
        (centred.origin.x, centred.size.width),
        (3.0 * COLUMN, 6.0 * COLUMN)
    );
    Ok(())
}

#[test]
fn a_cell_that_names_its_column_still_takes_the_full_width_in_a_compact_window() -> TestResult {
    let _entered = Runtime::new().enter();
    let ui = page(360.0, || {
        view! {
            <column_grid w=360.0>
                <grid_cell span=8 start=3 test_id="centred" h=20.0/>
            </column_grid>
        }
    });
    let centred = ui.bounds_of("centred").ok_or("centred")?;
    assert_eq!((centred.origin.x, centred.size.width), (0.0, 360.0));
    Ok(())
}

#[test]
fn a_reading_column_is_the_widest_centred_span_whose_lines_stay_readable() {
    let fits = |width: f32| move |span: u16| f32::from(span) * width / 12.0 - GRID_GUTTER.px();
    for width in [640.0, 900.0, 1248.0, 1600.0, 2400.0] {
        let span = reading_span(width, 720.0);
        assert_eq!(span % 2, 0, "{width} gives an even span so it centres");
        assert!(
            fits(width)(span) <= 720.0 || span == 4,
            "{width} gives {span}, too wide"
        );
        assert!(
            span == GRID_COLUMNS || fits(width)(span + 2) > 720.0,
            "{width} gives {span}, narrower than it need be"
        );
    }
    assert_eq!(
        reading_span(5000.0, 720.0),
        4,
        "it never narrows past a third of the grid"
    );
}

#[test]
fn a_reading_cell_centres_its_span_in_the_grid() -> TestResult {
    let _entered = Runtime::new().enter();
    let span = signal(8u16);
    let mut ui = page(WIDTH, move || {
        view! {
            <column_grid w=WIDTH>
                <reading_cell span={move || span.get()} test_id="reading" h=20.0/>
            </column_grid>
        }
    });
    let reading = ui.bounds_of("reading").ok_or("reading")?;
    assert_eq!(
        (reading.origin.x, reading.size.width),
        (2.0 * COLUMN, 8.0 * COLUMN)
    );
    span.set(10);
    ui.advance(0.0);
    let reading = ui.bounds_of("reading").ok_or("reading")?;
    assert_eq!(
        (reading.origin.x, reading.size.width),
        (COLUMN, 10.0 * COLUMN)
    );
    Ok(())
}

#[test]
fn the_overlay_draws_one_stripe_per_column_and_lets_clicks_through() -> TestResult {
    let _entered = Runtime::new().enter();
    let visible = signal(false);
    let clicks = signal(0);
    let mut ui = page(WIDTH, move || {
        view! {
            <div relative w=WIDTH h=100.0>
                <div test_id="target" size_full on_click={move |_| clicks.update(|clicks| *clicks += 1)}/>
                <grid_overlay visible={move || visible.get()}/>
            </div>
        }
    });
    assert_eq!(ui.bounds_of("grid-overlay"), None);
    visible.set(true);
    ui.advance(0.0);
    let overlay = ui.bounds_of("grid-overlay").ok_or("overlay")?;
    assert_eq!(overlay.size.width, WIDTH);
    let background = ui.pixel(4, 50)?;
    let stripes = (0..GRID_COLUMNS)
        .map(|column| ui.pixel((f32::from(column) * COLUMN + COLUMN / 2.0) as u32, 50))
        .collect::<Result<Vec<_>, _>>()?;
    assert!(stripes.iter().all(|stripe| *stripe != background));
    ui.click_on("target")?;
    assert_eq!(clicks.get(), 1);
    Ok(())
}

const WIDTH: f32 = 1248.0;
const COLUMN: f32 = WIDTH / 12.0;

fn page<T: View>(width: f32, root: impl FnOnce() -> T) -> TestUi {
    component_test().size(width, 200.0).build(root)
}

fn content(name: &'static str) -> Div {
    view! { <div test_id=name h=20.0/> }
}
