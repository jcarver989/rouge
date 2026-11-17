use rouge_core::testing::TestResult;
use rouge_core::{Styled, div, text, view};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{
    ButtonVariant, CheckState, ControlSize, IconName, PickerItem, SelectionState, button, checkbox,
    icon_button, list_row, picker_list, radio, section_header, segment, segmented_control,
    separator, slider, tabs, text_input, toggle, tree_row,
};

#[test]
fn widgets_take_their_required_value_as_an_attribute_and_the_rest_as_methods() -> TestResult {
    let _rt = Runtime::new().enter();
    let (on, size, volume, choice) = (signal(true), signal(1usize), signal(0.5f32), signal(0));
    let check = signal(CheckState::Indeterminate);
    let ui = component_test().size(900.0, 600.0).build(move || {
        view! {
            <div flex col>
                <button label="Save" test_id="button" variant=ButtonVariant::Primary
                        control_size=ControlSize::Small disabled=on selected=true tooltip="Tip"
                        on_click={|| {}} w=80.0/>
                <icon_button icon=IconName::Plus label="Add" test_id="icon-button" disabled=on selected=on/>
                <checkbox checked=check test_id="checkbox" disabled=on/>
                <toggle checked=on disabled=on w=40.0/>
                <radio selected=choice value=1 label="One" disabled=on/>
                <segmented_control test_id="segments">
                    <segment label="Segment" test_id="segment" icon=IconName::Check detail="Detail"
                             selected=on disabled=on/>
                </segmented_control>
                <section_header title="Section" test_id="section" expanded=on detail="Detail" actions_always_visible/>
                <list_row test_id="row" depth=1 selection=SelectionState::Selected><text>"Row"</text></list_row>
                <tree_row label="Tree" test_id="tree" depth=1 expanded=on icon=IconName::Folder/>
                <slider value=volume range={(0.0, 1.0)} step=0.1 w=120.0/>
                <separator vertical/>
                <text_input value={signal(String::new())} placeholder="Name" multiline min_rows=2/>
                <picker_list items={vec![PickerItem::new("item", "Item")]} test_id="picker" highlighted=Some(0)/>
                <tabs selected=size tab=("A", || div())/>
            </div>
        }
    });
    for id in [
        "button",
        "icon-button",
        "checkbox",
        "segment",
        "segment-detail",
        "section",
        "row",
        "tree",
        "picker",
    ] {
        assert!(ui.bounds_of(id).is_some(), "{id} mounted");
    }
    Ok(())
}
