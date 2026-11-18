use rouge_core::testing::{TestResult, assert_near};
use rouge_core::view;
use rouge_core::{Color, Styled, UiPalette, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{field, text_input};

#[test]
fn an_error_replaces_the_hint_and_turns_the_control_border_destructive() -> TestResult {
    let _entered = Runtime::new().enter();
    let error = signal(None::<String>);
    let mut ui = component_test()
        .size(300.0, 120.0)
        .clear(Color::WHITE)
        .build(move || {
            let name = signal(String::new());
            let input = view! {
                <div test_id="input">
                    <text_input value=name w=200.0/>
                </div>
            };
            view! {
                <div p=10.0>
                    <field label="Playlist name".to_owned() test_id="name" hint="Shown on your profile" error=error>
                        {input}
                    </field>
                </div>
            }
        });
    let palette = UiPalette::light().map(Color::from);
    let input = ui.rect_of("input")?;
    let edge = (input.origin.x as u32, input.center().y as u32);
    let label = ui
        .text_bounds("Playlist name")
        .ok_or("the label is drawn")?;
    assert!(
        label.max().y <= input.origin.y,
        "the label sits above the control"
    );
    assert!(ui.text_bounds("Shown on your profile").is_some());
    assert_near(
        ui.pixel(edge.0, edge.1)?,
        palette.border.to_rgba8(),
        "resting border",
    );
    let created = ui.stats().elements_created;
    error.set(Some("A playlist needs a name".into()));
    ui.run(0.3);
    assert!(ui.text_bounds("Shown on your profile").is_none());
    let note = ui
        .text_bounds("A playlist needs a name")
        .ok_or("the error is drawn")?;
    assert!(
        note.origin.y >= input.max().y,
        "the error sits below the control"
    );
    assert_near(
        ui.pixel(edge.0, edge.1)?,
        palette.destructive.to_rgba8(),
        "destructive border",
    );
    assert_eq!(ui.stats().elements_created, created, "no remount");
    error.set(None);
    ui.run(0.3);
    assert!(ui.text_bounds("Shown on your profile").is_some());
    ui.press("tab");
    ui.run(0.3);
    assert_near(
        ui.pixel(edge.0, edge.1)?,
        palette.accent.to_rgba8(),
        "focus border",
    );
    Ok(())
}
