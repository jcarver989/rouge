use rouge_core::testing::TestResult;
use rouge_core::{Color, Styled, div};
use rouge_reactive::Runtime;
use rouge_widgets::testing::component_test;
use rouge_widgets::*;
use std::fs;
use std::path::Path;

#[test]
fn bundled_faces_and_lucide_paths_render() -> TestResult {
    let _entered = Runtime::new().enter();
    assert_eq!(bundled_fonts().count(), 8);
    let mut ui =
        component_test().build(|| div().child(Icon::new(IconName::Plus, Color::WHITE).size(48.0)));
    assert_eq!(ui.pixel(24, 24)?, [255; 4]);
    assert_eq!(ui.pixel(4, 4)?, [0; 4]);
    Ok(())
}

#[test]
fn every_icon_stays_inside_its_box() -> TestResult {
    let _entered = Runtime::new().enter();
    for &name in IconName::ALL {
        let mut ui = component_test().size(96.0, 96.0).build(|| {
            div()
                .size_full()
                .p(24.0)
                .child(Icon::new(name, Color::WHITE).size(48.0))
        });
        let image = ui.render()?;
        let outside = (0..96)
            .flat_map(|y| (0..96).map(move |x| (x, y)))
            .filter(|&(x, y)| !(20..76).contains(&x) || !(20..76).contains(&y))
            .find(|&(x, y)| image.pixel(x, y)[3] != 0);
        assert_eq!(outside, None, "{name:?} paints outside its box");
    }
    Ok(())
}

#[test]
fn every_icon_path_parses_to_the_end() -> TestResult {
    let icons = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
    let mut truncated = Vec::new();
    for entry in fs::read_dir(icons)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "path")
            && zeno::validate_svg(fs::read_to_string(&path)?.trim()).is_err()
        {
            truncated.extend(
                path.file_stem()
                    .map(|name| name.to_string_lossy().into_owned()),
            );
        }
    }
    truncated.sort();
    assert_eq!(truncated, Vec::<String>::new());
    Ok(())
}
