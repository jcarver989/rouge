use rouge_core::testing::TestResult;
use rouge_core::{Color, Styled, View, div, text, view};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::*;

#[test]
fn full_width_buttons_rows_and_segments_activate_only_enabled_choices() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, calls) = component_test().size(440.0, 320.0).build_with(|| {
        let calls = signal(Vec::new());
        let view = view! {
            <div flex col p=12.0 w=340.0 material=Material::Glass>
                <button test_id="primary" label="Primary" variant=ButtonVariant::Primary w_full
                        on_click={move || calls.update(|v| v.push(1))}/>
                <list_row test_id="row" depth=2 selection=SelectionState::Selected
                          on_click={move || calls.update(|v| v.push(2))}>
                    <text>"A row"</text>
                </list_row>
                <segmented_control test_id="segments">
                    <segment test_id="enabled" label="Enabled" selected=true on_click={move || calls.update(|v| v.push(3))}/>
                    <segment test_id="disabled" label="Disabled" icon=IconName::Check detail="Extra" disabled=true
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
            <icon_button test_id="action" icon=IconName::Plus label="Add" on_click={move || counts.update(|v| v.0 += 1)}/>
        };
        let view = view! {
            <div>
                <section_header test_id="section" title="Section" expanded=true detail="Details" badge=badge.into_any()
                                action=action actions_always_visible on_toggle={move || counts.update(|v| v.1 += 1)}/>
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
fn checked_styles_apply_while_a_checkbox_is_checked() -> TestResult {
    let _entered = Runtime::new().enter();
    let checked = signal(false);
    let mut ui = component_test().size(120.0, 120.0).build(move || {
        view! { <checkbox test_id="check" checked size=12.0 checked:size=20.0/> }
    });
    assert_eq!(ui.rect_of("check")?.size.width, 12.0);
    ui.click_on("check")?;
    assert!(checked.get());
    assert_eq!(ui.rect_of("check")?.size.width, 20.0);
    Ok(())
}

#[test]
fn an_icon_button_paints_its_icon_in_the_given_color() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(64.0, 64.0).build(|| {
        view! {
            <icon_button icon=IconName::Plus label="Add" color=Color::RED size=64.0 icon_size=64.0/>
        }
    });
    ui.advance(0.0);
    let [red, green, blue, _] = ui.pixel(32, 32)?;
    assert!(
        red > 200 && green < 60 && blue < 60,
        "got {red} {green} {blue}"
    );
    Ok(())
}

#[test]
fn the_segment_indicator_slides_to_the_selected_segment() -> TestResult {
    let _entered = Runtime::new().enter();
    let selected = signal(0);
    let mut ui = component_test().size(320.0, 120.0).build(move || {
        view! {
            <segmented_control test_id="segments">
                <segment test_id="first" label="First" w=80.0 selected={move || selected.get() == 0}/>
                <segment test_id="second" label="Second" w=80.0 selected={move || selected.get() == 1}/>
            </segmented_control>
        }
    });
    ui.run(0.5);
    assert_eq!(ui.rect_of("segments-indicator")?, ui.rect_of("first")?);
    selected.set(1);
    ui.run(1.0);
    assert_eq!(ui.rect_of("segments-indicator")?, ui.rect_of("second")?);
    Ok(())
}

#[test]
fn the_segment_indicator_hides_without_a_selection_and_returns_with_one() -> TestResult {
    let _entered = Runtime::new().enter();
    let selected = signal(Some(0));
    let mut ui = component_test().size(320.0, 120.0).build(move || {
        view! {
            <segmented_control test_id="segments" indicator:bg=Color::RED>
                <segment test_id="first" label="First" w=80.0 selected={move || selected.get() == Some(0)}/>
                <segment test_id="second" label="Second" w=80.0 selected={move || selected.get() == Some(1)}/>
            </segmented_control>
        }
    });
    ui.run(0.5);
    let first = ui.rect_of("first")?;
    let (x, y) = (first.origin.x as u32 + 6, first.center().y as u32);
    assert_eq!(ui.rect_of("segments-indicator")?, first);
    assert!(is_red(ui.pixel(x, y)?));
    selected.set(None);
    ui.run(0.5);
    assert!(!is_red(ui.pixel(x, y)?));
    selected.set(Some(1));
    ui.run(1.0);
    assert_eq!(ui.rect_of("segments-indicator")?, ui.rect_of("second")?);
    assert!(!is_red(ui.pixel(x, y)?));
    Ok(())
}

#[test]
fn checkboxes_keep_their_size_and_toggle_without_activating_their_row() -> TestResult {
    let _entered = Runtime::new().enter();
    let (state, plain, disabled, rows) = (
        signal(CheckState::Indeterminate),
        signal(false),
        signal(true),
        signal(0),
    );
    let mut ui = component_test().size(440.0, 320.0).build(move || {
        view! {
            <list_row test_id="row" on_click={move || rows.update(|count| *count += 1)}>
                <text>"file.rs"</text>
                <checkbox test_id="check" checked=state/>
                <checkbox test_id="disabled" checked=plain disabled=disabled/>
            </list_row>
        }
    });
    let bounds = ui.rect_of("check")?;
    assert_eq!((bounds.size.width, bounds.size.height), (12.0, 12.0));
    ui.click_on("check")?;
    assert_eq!((state.get(), rows.get()), (CheckState::Checked, 0));
    ui.press("space");
    assert_eq!(state.get(), CheckState::Unchecked);
    ui.click_on("disabled")?;
    assert_eq!(
        (plain.get(), rows.get()),
        (false, 1),
        "the click falls through to the row"
    );
    disabled.set(false);
    ui.advance(0.0);
    ui.click_on("disabled")?;
    assert_eq!((plain.get(), rows.get()), (true, 1));
    Ok(())
}

fn is_red([red, green, blue, _]: [u8; 4]) -> bool {
    red > 200 && green < 60 && blue < 60
}
