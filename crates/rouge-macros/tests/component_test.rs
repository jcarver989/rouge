use rouge_core::testing::{TestResult, TestUi};
use rouge_core::{Color, Div, IntoProp, Point, Prop, Styled, View, component, div, text, view};
use rouge_reactive::{Runtime, signal};

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
    let mut ui = TestUi::new(|| {
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
