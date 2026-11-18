#![cfg(not(target_arch = "wasm32"))]

#[expect(dead_code, unused_imports)]
#[path = "../examples/shared/gallery/mod.rs"]
mod gallery;

use rouge::core::Rect;
use rouge::core::testing::TestResult;
use rouge::reactive::Runtime;
use rouge::widgets::testing::component_test;
use rouge::widgets::tokens::{GRID_COLUMNS, UNIT};

const WINDOW: (f32, f32) = (1296.0, 4000.0);

#[test]
fn every_gallery_cell_starts_on_a_column_and_sits_on_the_unit() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test()
        .size(WINDOW.0, WINDOW.1)
        .build(gallery::gallery);
    ui.run(1.0);
    let page = ui.bounds_of("page").ok_or("page")?;
    let column = page.size.width / f32::from(GRID_COLUMNS);
    let cells: Vec<(String, Rect)> = ui
        .named_bounds()
        .into_iter()
        .filter(|(name, _)| name.starts_with("cell-"))
        .collect();
    assert!(cells.len() > 30, "found {} cells", cells.len());
    let astray: Vec<String> = cells
        .into_iter()
        .filter(|(_, bounds)| {
            let (x, y) = (
                bounds.origin.x - page.origin.x,
                bounds.origin.y - page.origin.y,
            );
            x % column != 0.0
                || bounds.size.width % column != 0.0
                || y % UNIT != 0.0
                || bounds.size.height % UNIT != 0.0
        })
        .map(|(name, bounds)| {
            format!(
                "{name}: x {} y {} w {} h {}",
                bounds.origin.x - page.origin.x,
                bounds.origin.y - page.origin.y,
                bounds.size.width,
                bounds.size.height
            )
        })
        .collect();
    assert!(astray.is_empty(), "off the grid: {astray:#?}");
    Ok(())
}
