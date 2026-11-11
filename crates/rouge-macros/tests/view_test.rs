use rouge_core::testing::{BLUE, LINEAR, RED, TestResult, TestUi, assert_near};
use rouge_core::{Color, Point, Pose, Styled, div, each, show, text, view};
use rouge_reactive::{Runtime, signal};

#[test]
fn tags_call_their_constructor_and_attributes_call_builder_methods() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        view! {
            <div flex size_full>
                <div w=10.0 h=10.0/>
                <div w={10.0 + 10.0} h=10.0></div>
            </div>
        }
    });
    assert_eq!(row_widths(&mut ui), [10.0, 20.0]);
}

#[test]
fn string_children_become_text_and_holes_track_signals() -> TestResult {
    let _rt = Runtime::new().enter();
    let count = signal(1);
    let mut ui = TestUi::new(|| {
        view! {
            <div flex col size_full items_start>
                <div test_id="counted">"count {count}"</div>
                "plain"
            </div>
        }
    });
    assert_eq!(
        ui.stats().elements,
        5,
        "root, container, wrapper and two text leaves"
    );
    let before = ui.bounds_of("counted").ok_or("counted text")?.size.width;
    count.set(1_000_000);
    ui.update();
    let after = ui.bounds_of("counted").ok_or("counted text")?.size.width;
    assert!(after > before, "the hole re-renders: {before} -> {after}");
    assert_eq!(ui.stats().elements, 5, "the leaf is patched, not rebuilt");
    Ok(())
}

#[test]
fn lazy_children_are_built_by_the_component_and_methods_still_apply() {
    let _rt = Runtime::new().enter();
    let on = signal(true);
    let mut ui = TestUi::new(|| {
        view! {
            <div flex size_full>
                <show when={move || on.get()} otherwise={|| view! { <div w=70.0 h=10.0/> }}>
                    <div w=40.0 h=10.0/>
                </show>
            </div>
        }
    });
    assert_eq!(row_widths(&mut ui), [40.0]);
    on.set(false);
    ui.update();
    assert_eq!(row_widths(&mut ui), [70.0]);
}

#[test]
fn let_binds_the_value_a_component_passes_to_its_children() {
    let _rt = Runtime::new().enter();
    let items = signal(vec![10u32, 20, 30]);
    let mut ui = TestUi::new(|| {
        view! {
            <div flex size_full>
                <each items={move || items.get()} key={|width| *width} let:width>
                    <div h=10.0 w={width.get_untracked() as f32}/>
                </each>
            </div>
        }
    });
    assert_eq!(row_widths(&mut ui), [10.0, 20.0, 30.0]);
    items.set(vec![30, 10]);
    ui.update();
    assert_eq!(row_widths(&mut ui), [30.0, 10.0]);
}

#[test]
fn several_children_in_a_slot_are_siblings_without_a_wrapper() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        view! {
            <div flex size_full>
                <div w=5.0 h=10.0/>
                <show when={|| true}>
                    <div w=10.0 h=10.0/>
                    <div w=20.0 h=10.0/>
                </show>
                <div w=7.0 h=10.0/>
            </div>
        }
    });
    assert_eq!(row_widths(&mut ui), [5.0, 10.0, 20.0, 7.0]);
}

#[test]
fn several_top_level_nodes_form_a_fragment() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        let pair = view! {
            <div w=10.0 h=10.0/>
            <div w=20.0 h=10.0/>
        };
        div().flex().size_full().child(pair)
    });
    assert_eq!(row_widths(&mut ui), [10.0, 20.0]);
}

#[test]
fn blocks_insert_views_and_spreads_insert_iterators() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        let first = div().w(5.0).h(10.0);
        view! {
            <div flex size_full>
                {first}
                {..(1..=3).map(|i| view! { <div w={i as f32 * 10.0} h=10.0/> })}
            </div>
        }
    });
    assert_eq!(row_widths(&mut ui), [5.0, 10.0, 20.0, 30.0]);
}

#[test]
fn tuple_values_spread_into_method_arguments() -> TestResult {
    let _rt = Runtime::new().enter();
    let visible = signal(false);
    let mut ui = TestUi::new(|| {
        view! {
            <div size_full>
                <show when={move || visible.get()} enter=(Pose::new().translate_x(100.0), LINEAR)>
                    <div size=20.0 bg=Color::RED/>
                </show>
            </div>
        }
    });
    visible.set(true);
    ui.advance(0.0);
    assert_eq!(ui.first_x(RED, 5)?, 100, "starts at the enter pose");
    ui.run(1.05);
    assert_eq!(ui.first_x(RED, 5)?, 0);
    Ok(())
}

#[test]
fn namespaced_attributes_style_interaction_states() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        view! {
            <div size_full>
                <div test_id="swatch" size=50.0 bg=Color::RED hover:bg=Color::BLUE/>
            </div>
        }
    });
    assert_near(ui.pixel(25, 25)?, RED, "at rest");
    ui.hover_on("swatch")?;
    assert_near(ui.pixel(25, 25)?, BLUE, "hovered");
    Ok(())
}

#[test]
fn view_output_mixes_with_builder_code() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        let label = text("built by hand");
        div()
            .flex()
            .size_full()
            .child(view! { <div w=10.0 h=10.0/> })
            .child(show(|| true, || view! { <div w=20.0 h=10.0/> }))
            .child(each(
                || vec![30u32],
                |w| *w,
                |w| div().h(10.0).w(w.get() as f32),
            ))
            .child(view! { <div>{label}</div> })
    });
    assert_eq!(row_widths(&mut ui), [10.0, 20.0, 30.0]);
    assert_eq!(ui.stats().elements, 9);
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
