use rouge_core::testing::{TestResult, counter};
use rouge_core::{Color, Styled, UiPalette, div, show, view};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{button, modal};

#[test]
fn modal_covers_the_window_traps_focus_and_dismisses() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(true);
    let (behind, hit_behind) = counter();
    let (confirmed, confirm) = counter();
    let mut ui = component_test().size(600.0, 400.0).build(move || {
        let dialog = move || {
            let confirm = confirm.clone();
            view! {
                <modal id="dialog" title="Confirm" on_dismiss={move || open.set(false)}>
                    <div flex gap=8.0>
                        <button id="cancel" label="Cancel" keyboard_focusable on_click={move || open.set(false)}/>
                        <button id="ok" label="OK" keyboard_focusable on_click=confirm/>
                    </div>
                </modal>
            }
        };
        div()
            .size_full()
            .child(
                div()
                    .absolute()
                    .left(0.0)
                    .top(350.0)
                    .size(40.0)
                    .on_click(move |_| hit_behind()),
            )
            .child(show(move || open.get(), dialog))
    });
    ui.run(0.6);
    assert_ne!(
        ui.pixel(590, 390)?,
        UiPalette::light().map(Color::from).canvas.to_rgba8(),
        "the scrim dims the page"
    );
    ui.press("tab");
    ui.press("tab");
    ui.press("enter");
    assert_eq!(confirmed.get(), 1, "tab walks the dialog's buttons");
    ui.press("tab");
    ui.press("tab");
    ui.press("enter");
    assert_eq!(
        (open.get(), confirmed.get()),
        (true, 2),
        "tab wraps inside the dialog"
    );
    ui.press("escape");
    ui.run(0.6);
    assert!(!open.get(), "escape dismisses");
    open.set(true);
    ui.run(0.6);
    ui.click_at(20.0, 370.0);
    ui.run(0.6);
    assert_eq!(
        (open.get(), behind.get()),
        (false, 0),
        "a scrim click dismisses and never reaches the page"
    );
    Ok(())
}
