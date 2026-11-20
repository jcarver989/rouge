mod common;

use common::builder;
use rouge_core::testing::{RED, TestResult, TestUi};
use rouge_core::view;
use rouge_core::{Color, InputKind, LayoutTransition, Styled, View, div};
use rouge_reactive::{Runtime, Signal, keyed_list, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::{MotionScale, Pace, Space, UNIT};
use rouge_widgets::{
    ButtonVariant, CheckState, ControlSize, IconName, NoticeTone, badge, button, checkbox,
    code_input, field, icon_button, kbd, number_field, progress, radio, segment, segmented_control,
    select, skeleton, slider, tag_input, text_input, toggle,
};

#[test]
fn every_pace_settles_and_then_draws_no_more_frames() -> TestResult {
    for pace in PACES {
        let _entered = Runtime::new().enter();
        let (mut ui, x) = sliding(pace);
        x.set(100.0);
        ui.advance(0.0);
        assert!(ui.is_animating(), "{pace:?} animates");
        ui.run(1.5);
        assert!(!ui.is_animating(), "{pace:?} settles");
        assert_eq!(ui.compositor_frames(1.0)?, 0, "{pace:?} idles");
    }
    Ok(())
}

#[test]
fn exit_never_overshoots_and_press_springs_slightly_past_its_target() -> TestResult {
    for (pace, overshoots) in [(Pace::Exit, false), (Pace::Press, true)] {
        let _entered = Runtime::new().enter();
        let (mut ui, x) = sliding(pace);
        x.set(100.0);
        let mut furthest = 0;
        for _ in 0..60 {
            ui.advance(1.0 / 60.0);
            furthest = furthest.max(ui.first_x(RED, 10)?);
        }
        assert_eq!(furthest > 100, overshoots, "{pace:?} reached {furthest}");
    }
    Ok(())
}

#[test]
fn a_pace_drives_a_layout_transition_until_it_settles() -> TestResult {
    let _entered = Runtime::new().enter();
    assert_eq!(MotionScale::current(), MotionScale::Full);
    let wide = signal(false);
    let mut ui = builder().build(move || {
        view! {
            <div h=20.0 w={move || if wide.get() { 200.0 } else { 100.0 }} bg=Color::RED
                 layout_transition=LayoutTransition::new(Pace::Layout).size()/>
        }
    });
    wide.set(true);
    ui.advance(0.0);
    assert!(ui.is_animating());
    ui.run(1.5);
    assert!(!ui.is_animating());
    assert_eq!(ui.compositor_frames(1.0)?, 0);
    Ok(())
}

#[test]
fn without_motion_an_animated_widget_settles_in_one_frame() -> TestResult {
    let _entered = Runtime::new().enter();
    let checked = signal(false);
    let (mut ui, scale) = component_test().size(120.0, 60.0).build_with(move || {
        let scale = MotionScale::None.provide();
        let root = view! {
            <div p=10.0 flex>
                <toggle checked=checked test_id="toggle"/>
            </div>
        };
        (root, scale)
    });
    assert_eq!(scale.get(), MotionScale::None);
    let rest = ui.rect_of("toggle-thumb")?;
    checked.set(true);
    ui.advance(1.0 / 60.0);
    assert!(!ui.is_animating(), "the thumb lands at once");
    let image = ui.render()?;
    assert_eq!(ui.rect_of("toggle-thumb")?.size, rest.size);
    assert_eq!(ui.compositor_frames(1.0)?, 0);
    let (x, y) = (rest.center().x as u32, rest.center().y as u32);
    assert_ne!(
        image.pixel(x, y),
        [255; 4],
        "the thumb has left its resting place"
    );
    assert_eq!(image.pixel(x + 16, y), [255; 4], "and sits at the far end");
    Ok(())
}

#[test]
fn a_space_sets_padding_and_gaps_wherever_a_length_does() -> TestResult {
    let _entered = Runtime::new().enter();
    let ui = builder().build(|| {
        view! {
            <div flex col p=Space::X4 gap=Space::X2>
                <div test_id="first" size=20.0/>
                <div test_id="second" size=20.0/>
            </div>
        }
    });
    let first = ui.bounds_of("first").ok_or("first")?;
    let second = ui.bounds_of("second").ok_or("second")?;
    assert_eq!((first.origin.x, first.origin.y), (16.0, 16.0));
    assert_eq!(second.origin.y - first.max().y, 8.0);
    Ok(())
}

#[test]
fn every_control_is_a_whole_number_of_units_tall_and_starts_on_the_grid() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(480.0, 1200.0).build(catalogue);
    ui.run(0.5);
    let off_grid: Vec<String> = ui
        .named_bounds()
        .into_iter()
        .filter(|(name, _)| CONTROLS.contains(&name.as_str()))
        .filter(|(_, bounds)| bounds.origin.y % UNIT != 0.0 || bounds.size.height % UNIT != 0.0)
        .map(|(name, bounds)| {
            format!(
                "{name} at {} is {} tall",
                bounds.origin.y, bounds.size.height
            )
        })
        .collect();
    assert!(off_grid.is_empty(), "off the {UNIT} px grid: {off_grid:#?}");
    for control in CONTROLS {
        assert!(ui.bounds_of(control).is_some(), "{control} is mounted");
    }
    Ok(())
}

#[test]
fn on_touch_every_control_stays_on_the_grid_and_offers_a_44_px_target() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test()
        .size(480.0, 1600.0)
        .input(InputKind::Touch)
        .build(catalogue);
    ui.run(0.5);
    for control in CONTROLS {
        let bounds = ui.rect_of(control)?;
        assert_eq!(
            (bounds.origin.y % UNIT, bounds.size.height % UNIT),
            (0.0, 0.0),
            "{control} is on the grid: {bounds:?}"
        );
    }
    for control in [
        "iconed", "icon", "toggle", "mixed", "checked", "radio", "segments", "slider", "input",
        "number", "select",
    ] {
        let height = ui.rect_of(control)?.size.height;
        assert!(height >= 44.0, "{control} is {height} tall on touch");
    }
    assert_eq!(ui.rect_of("icon")?.size.width, 44.0);
    let thumb = ui.rect_of("toggle-thumb")?.size;
    assert_eq!((thumb.width, thumb.height), (28.0, 28.0));
    Ok(())
}

const PACES: [Pace; 6] = [
    Pace::Press,
    Pace::Move,
    Pace::Enter,
    Pace::Exit,
    Pace::Layout,
    Pace::Gesture,
];

const CONTROLS: [&str; 21] = [
    "small", "regular", "large", "iconed", "icon", "toggle", "mixed", "checked", "radio",
    "segments", "slider", "input", "field", "number", "tags", "code", "select", "badge", "kbd",
    "skeleton", "progress",
];

fn sliding(pace: Pace) -> (TestUi, Signal<f32>) {
    let x = signal(0.0);
    let ui = builder().build(move || {
        view! { <div size=20.0 bg=Color::RED translate_x=x transition=pace/> }
    });
    (ui, x)
}

fn catalogue() -> impl View {
    let (on, checked, state) = (
        signal(true),
        signal(true),
        signal(CheckState::Indeterminate),
    );
    let (quality, picked, volume) = (signal("high"), signal(0usize), signal(40.0));
    let (name, tempo, code) = (
        signal(String::from("Late night")),
        signal(96.0),
        signal(String::from("48")),
    );
    let tags = keyed_list(String::clone);
    tags.extend(["ambient", "vinyl"].map(str::to_owned));
    let sized = |id: &'static str, size: ControlSize| {
        view! {
            <button label="Add to queue".to_owned() test_id=id variant=ButtonVariant::Primary
                 control_size=size/>
        }
    };
    view! {
        <div flex col items_start gap=Space::X2 p=Space::X4 w=480.0>
            {sized("small", ControlSize::Small)}
            {sized("regular", ControlSize::Medium)}
            {sized("large", ControlSize::Large)}
            <button label="Delete".to_owned() test_id="iconed" icon=IconName::X
                 variant=ButtonVariant::Destructive/>
            <icon_button icon=IconName::Copy label="Copy link".to_owned() test_id="icon"/>
            <toggle checked=on test_id="toggle"/>
            <checkbox checked=state test_id="mixed"/>
            <checkbox checked=checked test_id="checked"/>
            <div test_id="radio">
                <radio selected=quality value="high" label="High"/>
            </div>
            <segmented_control test_id="segments">
                {..[
                    view! { <segment label="Grid".to_owned() test_id="grid" selected={|| true}/> },
                    view! { <segment label="List".to_owned() test_id="list" selected={|| false}/> },
                ]}
            </segmented_control>
            <slider value=volume test_id="slider" range={(0.0, 100.0)} w_full/>
            <text_input value=name test_id="input" w_full/>
            <field label="Playlist name".to_owned() test_id="field" hint="Shown on your profile">
                <text_input value=name w_full/>
            </field>
            <number_field value=tempo test_id="number" label="Tempo" suffix="bpm"/>
            <tag_input tags=tags test_id="tags" placeholder="Add a tag" w_full/>
            <code_input value=code length=6 test_id="code"/>
            <div test_id="select">
                <select value=picked options=[(0, "Lossless".to_owned()), (1, "High".to_owned())]/>
            </div>
            <badge tone=NoticeTone::Positive test_id="badge">{"Synced".to_owned()}</badge>
            <kbd test_id="kbd">{"K".to_owned()}</kbd>
            <skeleton test_id="skeleton" w=120.0/>
            <progress value={|| 0.5} test_id="progress" w=120.0/>
        </div>
    }
}
