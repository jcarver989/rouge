use rouge_core::testing::{TestResult, TestUi};
use rouge_core::view;
use rouge_core::{Div, Styled, View, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::{GRID_COLUMNS, GRID_GUTTER, GRID_MARGIN};
use rouge_widgets::{column_grid, grid_cell, grid_overlay};

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
