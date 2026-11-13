use rouge_core::testing::TestResult;
use rouge_core::{Color, Styled, div};
use rouge_reactive::Runtime;
use rouge_widgets::testing::component_test;
use rouge_widgets::*;

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
