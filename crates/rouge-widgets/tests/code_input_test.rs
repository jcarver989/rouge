use rouge_core::testing::{TestResult, TestUi, assert_near};
use rouge_core::view;
use rouge_core::{Color, Styled, UiPalette, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::code_input;
use rouge_widgets::testing::component_test;

#[test]
fn a_code_fills_its_cells_from_one_editable_and_reports_completion() -> TestResult {
    let _entered = Runtime::new().enter();
    let (code, done) = (signal(String::new()), signal(None::<String>));
    let mut ui = component_test()
        .size(320.0, 100.0)
        .clear(Color::WHITE)
        .build(move || {
            view! {
                <div p=20.0 flex>
                    <code_input value=code length=4 test_id="code"
                         on_complete={move |value: String| done.set(Some(value))}/>
                </div>
            }
        });
    let palette = UiPalette::light().map(Color::from);
    let edge = |ui: &mut TestUi, index: usize| {
        let cell = ui.rect_of(&format!("code-cell-{index}"))?;
        ui.pixel(cell.origin.x as u32 + 12, cell.max().y as u32 - 1)
    };
    assert_eq!(ui.rect_of("code-cell-0")?.size.height, 40.0);
    ui.click_on("code-cell-2")?;
    ui.run(0.5);
    assert_near(
        edge(&mut ui, 0)?,
        palette.accent.to_rgba8(),
        "the next cell is marked",
    );
    ui.type_text("12");
    ui.run(0.5);
    assert_eq!(code.get(), "12");
    assert_near(
        edge(&mut ui, 2)?,
        palette.accent.to_rgba8(),
        "the mark moves on",
    );
    assert_near(
        edge(&mut ui, 0)?,
        palette.border.to_rgba8(),
        "and leaves the first",
    );
    assert_eq!(done.get(), None);
    ui.type_text("3 456");
    ui.run(0.5);
    assert_eq!(
        code.get(),
        "1234",
        "pasted or typed text is cut to the length"
    );
    assert_eq!(done.get().as_deref(), Some("1234"));
    let (third, digit) = (
        ui.rect_of("code-cell-2")?,
        ui.text_bounds("3").ok_or("the digit is drawn")?,
    );
    assert!(
        third.contains(digit.center()),
        "each symbol is drawn in its cell"
    );
    ui.press("backspace");
    assert_eq!(code.get(), "123");
    ui.run(0.5);
    assert_eq!(ui.compositor_frames(1.0)?, 0);
    Ok(())
}
