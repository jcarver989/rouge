mod common;

use common::builder;
use rouge_core::testing::{TestError, TestResult};
use rouge_core::view;
use rouge_core::{Color, Styled, div};
use rouge_reactive::Runtime;
use rouge_widgets::{Material, MaterialExt};

#[test]
fn each_glass_lets_through_more_of_the_page_than_the_one_before() -> TestResult {
    let [frosted, lens, clear] = [Material::Glass, Material::Lens, Material::Clear].map(centre);
    let (frosted, lens, clear) = (frosted?, lens?, clear?);
    assert!(
        frosted[1] > lens[1] && lens[1] > clear[1],
        "tint fades from {frosted:?} through {lens:?} to {clear:?}"
    );
    assert!(clear[0] > 240 && clear[1] < 16, "clear glass adds no tint");
    Ok(())
}

#[test]
fn a_lens_bends_the_page_at_its_edge_and_clear_glass_keeps_its_middle_sharp() -> TestResult {
    let _entered = Runtime::new().enter();
    let striped = |material: Material| {
        let mut ui = builder().build(move || {
            view! {
                <div size_full flex>
                    <div w=100.0 h_full bg=Color::RED/>
                    <div flex_1 h_full bg=Color::BLUE/>
                    <div absolute left=40.0 top=40.0 w=120.0 h=80.0 material=material/>
                </div>
            }
        });
        Ok::<_, TestError>((ui.pixel(46, 80)?, ui.pixel(80, 80)?))
    };
    let (edge, middle) = striped(Material::Clear)?;
    assert!(middle[0] > 240 && middle[2] < 16, "the middle is untouched");
    let (lens_edge, _) = striped(Material::Lens)?;
    assert_ne!(edge, [255, 0, 0, 255], "the edge of clear glass is lit");
    assert_ne!(lens_edge, edge, "a lens bends its edge further");
    Ok(())
}

fn centre(material: Material) -> Result<[u8; 4], TestError> {
    let _entered = Runtime::new().enter();
    let mut ui = builder().build(move || {
        view! {
            <div size_full bg=Color::RED>
                <div absolute left=40.0 top=40.0 w=120.0 h=80.0 material=material/>
            </div>
        }
    });
    ui.pixel(100, 80)
}
