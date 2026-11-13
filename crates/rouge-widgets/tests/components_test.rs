use rouge_core::testing::TestResult;
use rouge_core::{Styled, View, div, text, view};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::*;

#[test]
fn full_width_buttons_rows_and_segments_activate_only_enabled_choices() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, calls) = component_test().size(440.0, 320.0).build_with(|| {
        let calls = signal(Vec::new());
        let view = view! {
            <div flex col p=12.0 w=340.0 material=Material::Floating>
                <button id="primary" label="Primary" variant=ButtonVariant::Primary full_width keyboard_focusable
                        on_click={move || calls.update(|v| v.push(1))}/>
                <list_row id="row" depth=2 state=SelectionState::Selected
                          on_click={move || calls.update(|v| v.push(2))}>
                    <text>"A row"</text>
                </list_row>
                <segmented_control id="segments">
                    <segment id="enabled" label="Enabled" selected on_click={move || calls.update(|v| v.push(3))}/>
                    <segment id="disabled" label="Disabled" icon=IconName::Check detail="Extra" disabled
                             on_click={move || calls.update(|v| v.push(4))}/>
                </segmented_control>
            </div>
        };
        (view, calls)
    });
    let button = ui.rect_of("primary")?;
    assert!(button.size.width > 300.0);
    ui.click_on("primary")?;
    ui.click_on("row")?;
    let detail = ui.center_of("disabled-detail")?;
    let disabled = ui.rect_of("disabled")?;
    assert!(disabled.contains(detail));
    ui.click_on("disabled")?;
    ui.click_on("enabled")?;
    ui.press("space");
    assert_eq!(calls.get(), vec![1, 2, 3, 3]);
    Ok(())
}

#[test]
fn section_action_and_disclosure_do_not_trigger_each_other() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, counts) = component_test().size(440.0, 320.0).build_with(|| {
        let counts = signal((0, 0));
        let badge = view! { <div test_id="badge"><text caption>"3"</text></div> };
        let action = view! {
            <icon_button id="action" icon=IconName::Plus label="Add" on_click={move || counts.update(|v| v.0 += 1)}/>
        };
        let view = view! {
            <div>
                <section_header id="section" title="Section" expanded=true detail="Details" badge=badge.into_any()
                                action actions_always_visible on_toggle={move || counts.update(|v| v.1 += 1)}/>
                <empty_state icon=IconName::File title="No files" detail="Nothing to review"/>
                <action_bar><notification message="Not saved" tone=NoticeTone::Error/></action_bar>
            </div>
        };
        (view, counts)
    });
    ui.click_on("action")?;
    assert_eq!(counts.get(), (1, 0));
    ui.click_on("section-toggle")?;
    assert_eq!(counts.get(), (1, 1));
    assert!(ui.bounds_of("badge").is_some());
    Ok(())
}

#[test]
fn checkboxes_keep_their_size_and_toggle_without_activating_their_row() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, counts) = component_test().size(440.0, 320.0).build_with(|| {
        let counts = signal((0, 0));
        let view = view! {
            <list_row id="row" on_click={move || counts.update(|v| v.1 += 1)}>
                <text>"file.rs"</text>
                <checkbox id="check" state=CheckState::Indeterminate on_toggle={move || counts.update(|v| v.0 += 1)}/>
                <checkbox id="disabled" state=CheckState::Checked disabled
                          on_toggle={move || counts.update(|v| v.0 += 10)}/>
            </list_row>
        };
        (view, counts)
    });
    let bounds = ui.rect_of("check")?;
    assert_eq!((bounds.size.width, bounds.size.height), (12.0, 12.0));
    ui.click_on("check")?;
    assert_eq!(counts.get(), (1, 0));
    ui.press("space");
    assert_eq!(counts.get(), (2, 0));
    ui.click_on("disabled")?;
    assert_eq!(counts.get(), (2, 1));
    Ok(())
}
