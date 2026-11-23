#![cfg(not(target_arch = "wasm32"))]

#[expect(dead_code, unused_imports)]
#[path = "../examples/shared/gallery/mod.rs"]
mod gallery;

use rouge::core::Rect;
use rouge::core::access::{Action, Role};
use rouge::core::testing::TestResult;
use rouge::reactive::Runtime;
use rouge::widgets::testing::component_test;
use rouge::widgets::tokens::{GRID_COLUMNS, UNIT};

const WINDOW: (f32, f32) = (1296.0, 4000.0);
const WHOLE_PAGE: f32 = 30000.0;

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

#[test]
fn every_interactive_gallery_element_has_a_role_and_a_name() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test()
        .size(WINDOW.0, WHOLE_PAGE)
        .build(gallery::gallery);
    ui.run(1.0);
    let nodes = ui.access_nodes();
    let interactive: Vec<_> = nodes
        .into_iter()
        .filter(|node| {
            node.actions.contains(&Action::Click) || node.actions.contains(&Action::Focus)
        })
        .collect();
    assert!(
        interactive.len() > 150,
        "found {} controls",
        interactive.len()
    );
    let unnamed: Vec<String> = interactive
        .into_iter()
        .filter(|node| node.role == Role::Unknown || node.name.is_empty())
        .map(|node| format!("{:?} {:?} at {:?}", node.role, node.name, node.bounds))
        .collect();
    assert!(unnamed.is_empty(), "without a role or a name: {unnamed:#?}");
    Ok(())
}
