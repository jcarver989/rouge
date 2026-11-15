use rouge_core::testing::{TestResult, TestUi};
use rouge_core::{Color, Div, IntoProp, Point, Prop, Styled, View, component, div, text, view};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{
    ButtonVariant, CheckState, ControlSize, IconName, PickerItem, SelectionState, button, checkbox,
    icon_button, list_row, picker_list, radio, section_header, segment, segmented_control,
    separator, slider, tabs, text_input, toggle, tree_row,
};
use std::panic::Location;

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

#[test]
fn attributes_matching_parameters_become_arguments_in_declaration_order() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        view! {
            <div flex size_full>
                <bar height=10.0 width=30.0/>
                <bar width=20.0 height=10.0 ml=5.0 test_id="second"/>
            </div>
        }
    });
    assert_eq!(row_widths(&mut ui), [30.0]);
    let second = ui.bounds_of("second").ok_or("second bar")?;
    assert_eq!((second.origin.x, second.size.width), (35.0, 20.0));
    Ok(())
}

#[test]
fn a_bare_attribute_naming_a_parameter_is_shorthand_for_a_variable() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        let (width, height) = (25.0, 10.0);
        view! {
            <div flex size_full>
                <bar width height/>
            </div>
        }
    });
    assert_eq!(row_widths(&mut ui), [25.0]);
}

#[test]
fn view_children_are_passed_as_one_argument() -> TestResult {
    let _rt = Runtime::new().enter();
    let ui = TestUi::new(|| {
        view! {
            <div flex size_full items_start>
                <labeled name="pair">
                    <bar width=10.0 height=10.0/>
                    <bar width=20.0 height=10.0/>
                </labeled>
            </div>
        }
    });
    let pair = ui.bounds_of("pair").ok_or("labeled container")?;
    assert_eq!(pair.size.width, 30.0, "both children land inside");
    Ok(())
}

#[test]
fn closure_children_receive_let_bindings() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        view! {
            <repeat times=3 let:index>
                <bar width={(index + 1) as f32 * 10.0} height=10.0/>
            </repeat>
        }
    });
    assert_eq!(row_widths(&mut ui), [10.0, 20.0, 30.0]);
}

#[test]
fn value_children_pass_a_single_child_as_is() -> TestResult {
    let _rt = Runtime::new().enter();
    let name = signal("short");
    let mut ui = TestUi::new(|| {
        view! {
            <div flex size_full items_start>
                <caption>"{name} caption"</caption>
            </div>
        }
    });
    let before = ui.bounds_of("caption").ok_or("caption")?.size.width;
    name.set("a much longer");
    ui.update();
    let after = ui.bounds_of("caption").ok_or("caption")?.size.width;
    assert!(after > before);
    Ok(())
}

#[test]
fn components_without_children_parameters_receive_content_as_child_views() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        view! {
            <row>
                <bar width=10.0 height=10.0/>
                <bar width=20.0 height=10.0/>
            </row>
        }
    });
    assert_eq!(row_widths(&mut ui), [10.0, 20.0]);
}

#[test]
fn components_are_imported_like_functions_from_other_modules() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        view! {
            <row>
                <parts::square side=10.0/>
                <square side=10.0/>
            </row>
        }
    });
    assert_eq!(row_widths(&mut ui), [10.0, 10.0]);
}

#[test]
fn optional_and_defaulted_parameters_may_be_left_out() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        let known = Some(15.0);
        view! {
            <row>
                <block/>
                <block width=20.0/>
                <block width?=known wide/>
                <block label="named" tall/>
                <tinted shade={move || Color::RED}/>
                <tinted/>
                <owned>"converted"</owned>
            </row>
        }
    });
    assert_eq!(row_widths(&mut ui), [10.0, 20.0, 30.0]);
    assert!(ui.bounds_of("tinted").is_some());
    assert!(ui.bounds_of("converted").is_some());
    assert_eq!(ui.bounds_of("named").map(|b| b.size.height), Some(20.0));
}

#[test]
fn attributes_apply_conditionally_with_question_and_when() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        let (wide, none) = (Some(40.0), None::<f32>);
        view! {
            <div flex size_full items_start>
                <div h=10.0 w=10.0 flex_none w?=wide/>
                <div h=10.0 w=10.0 flex_none w?=none/>
                <div h=10.0 w=10.0 flex_none when:w_full=false/>
                <div h=10.0 w=10.0 flex_none when:w=(true, 15.0) when:h=(false, 99.0)/>
                <div h=10.0 w=10.0 when:flex_none=true when:w_full=true/>
            </div>
        }
    });
    assert_eq!(row_widths(&mut ui), [40.0, 10.0, 10.0, 15.0, 200.0]);
}

#[test]
fn branches_of_different_view_types_are_erased() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        let (first, second, kind) = (true, false, 2);
        view! {
            <div flex size_full items_start>
                {if first { bar(10.0, 10.0) } else { text("no") }}
                {if second { bar(99.0, 10.0) }}
                {match kind {
                    1 => text("one"),
                    _ => bar(30.0, 10.0),
                }}
                {Some(bar(5.0, 10.0))}
                {()}
            </div>
        }
    });
    assert_eq!(row_widths(&mut ui), [10.0, 30.0, 5.0]);
}

#[test]
fn components_see_their_call_site_as_the_caller() {
    assert_eq!(located().line(), line!());
}

mod parts {
    use rouge_core::{Div, Styled, component, div};

    #[component]
    pub(crate) fn square(side: f32) -> Div {
        div().size(side)
    }
}

use parts::square;

#[component]
fn bar(width: f32, height: f32) -> Div {
    div().w(width).h(height)
}

#[component]
fn labeled(name: &'static str, children: impl View) -> Div {
    div().flex().test_id(name).child(children)
}

#[component]
fn repeat<T: View>(times: usize, children: impl Fn(usize) -> T + 'static) -> Div {
    div().flex().size_full().children((0..times).map(children))
}

#[component]
fn caption<U>(children: impl IntoProp<String, U>) -> Div {
    div().test_id("caption").child(text(children))
}

#[component]
fn row() -> Div {
    div().flex().size_full()
}

#[component]
fn located() -> &'static Location<'static> {
    Location::caller()
}

fn row_widths(ui: &mut TestUi) -> Vec<f32> {
    let mut widths = Vec::new();
    let mut x = 0.5;
    while let Some(b) = ui
        .bounds_at(Point::new(x, 5.0))
        .filter(|b| b.size.height == 10.0)
    {
        widths.push(b.size.width);
        x = b.origin.x + b.size.width + 0.5;
    }
    widths
}

#[component]
fn block(
    width: Option<f32>,
    #[prop(default)] wide: bool,
    #[prop(default)] tall: bool,
    #[prop(into)] label: Option<String>,
) -> Div {
    let width = width.unwrap_or(10.0) * if wide { 2.0 } else { 1.0 };
    let block = div().w(width).h(if tall { 20.0 } else { 10.0 }).flex_none();
    match label {
        Some(label) => block.test_id(label),
        None => block,
    }
}

#[component]
fn tinted(#[prop(reactive)] shade: Option<Prop<Color>>) -> Div {
    let block = div().size(10.0).flex_none();
    match shade {
        Some(shade) => block.test_id("tinted").bg(move || shade.get()),
        None => block,
    }
}

#[component]
fn owned(#[prop(into)] children: String) -> Div {
    div().test_id(children)
}
