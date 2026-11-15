use rouge_core::testing::TestResult;
use rouge_core::{Styled, div, view};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::*;

#[test]
fn nested_tree_keeps_full_hit_width_and_separates_toggle_from_selection() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, counts) = component_test().build_with(|| {
        let counts = signal((0, 0));
        let root = view! {
            <div flex col w_full>
                <tree_row test_id="root" label="Root" expanded=true/>
                <tree_row test_id="child" label="Child" depth=2 expanded=false icon=IconName::Folder
                          selection=SelectionState::Selected on_click={move || counts.update(|v| v.0 += 1)}
                          on_toggle={move || counts.update(|v| v.1 += 1)}/>
            </div>
        };
        (root, counts)
    });
    assert!(ui.center_of("child-toggle")?.x > ui.center_of("root-toggle")?.x + 20.0);
    ui.click_on("child-toggle")?;
    let child = ui.center_of("child")?;
    ui.click_at(190.0, child.y);
    assert_eq!(counts.get(), (1, 1));
    Ok(())
}

#[test]
fn trailing_controls_activate_without_selecting_the_row() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, (clicks, checked)) = component_test().build_with(|| {
        let (clicks, checked) = (signal(0), signal(false));
        let root = view! {
            <div flex col w_full>
                <tree_row test_id="row" label="Row" on_click={move || clicks.update(|v| *v += 1)}>
                    <checkbox checked test_id="row-check"/>
                </tree_row>
            </div>
        };
        (root, (clicks, checked))
    });
    ui.click_on("row-check")?;
    assert_eq!((clicks.get(), checked.get()), (0, true));
    ui.click_on("row-label")?;
    assert_eq!((clicks.get(), checked.get()), (1, true));
    Ok(())
}
