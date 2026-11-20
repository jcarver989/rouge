use rouge_core::testing::{TestResult, TestUi};
use rouge_core::view;
use rouge_core::{InputKind, Point, Styled, div};
use rouge_reactive::{Runtime, Signal, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{alert, row_action};

#[test]
fn an_alert_is_a_small_centred_panel_on_a_handheld_that_only_its_buttons_answer() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, open, deleted) = confirm(InputKind::Touch);
    assert_eq!(ui.bounds_of("confirm"), None);
    open.set(true);
    ui.run(1.0);
    let panel = ui.rect_of("confirm")?;
    assert_eq!(panel.size.width, 272.0);
    assert!(
        (panel.center().x - 195.0).abs() <= 0.5 && (panel.center().y - 350.0).abs() <= 0.5,
        "centred rather than a sheet: {panel:?}"
    );
    let (cancel, delete) = (
        ui.rect_of("confirm-cancel")?,
        ui.rect_of("confirm-action-0")?,
    );
    assert_eq!(cancel.size, delete.size, "two answers share a row");
    assert!(cancel.max().x <= delete.origin.x && cancel.size.height == 44.0);
    assert!(ui.text_bounds("This can't be undone.").is_some());
    ui.tap(Point::new(195.0, 80.0));
    ui.run(0.5);
    assert!(open.get(), "a press outside does not answer for the user");
    ui.tap_on("confirm-action-0")?;
    assert_eq!((open.get(), deleted.get()), (false, 1));
    ui.run(1.0);
    assert_eq!(ui.bounds_of("confirm"), None);
    Ok(())
}

#[test]
fn escape_cancels_and_the_page_beneath_hears_nothing_while_an_alert_is_up() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, open, deleted) = confirm(InputKind::Pointer);
    open.set(true);
    ui.run(1.0);
    ui.click_at(20.0, 20.0);
    assert_eq!(deleted.get(), 0, "the page's own button is covered");
    ui.press("escape");
    assert!(!open.get(), "Escape picks the cancel button");
    ui.run(1.0);
    ui.click_at(20.0, 20.0);
    assert_eq!(deleted.get(), 100, "the page is reachable again");
    Ok(())
}

#[test]
fn three_answers_stack_with_cancel_last() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(true);
    let mut ui = component_test().size(390.0, 700.0).build(move || {
        view! {
            <div size_full>
                <alert open=open title="Unsaved changes" test_id="save" action={row_action("Save", || {})}
                     action={row_action("Discard", || {}).destructive()} cancel="Keep editing"/>
            </div>
        }
    });
    ui.run(1.0);
    let (save, discard, cancel) = (
        ui.rect_of("save-action-0")?,
        ui.rect_of("save-action-1")?,
        ui.rect_of("save-cancel")?,
    );
    assert!(save.max().y <= discard.origin.y && discard.max().y <= cancel.origin.y);
    assert_eq!(save.size.width, cancel.size.width);
    Ok(())
}

fn confirm(input: InputKind) -> (TestUi, Signal<bool>, Signal<u32>) {
    let (open, deleted) = (signal(false), signal(0));
    let ui = component_test()
        .size(390.0, 700.0)
        .input(input)
        .build(move || {
            view! {
                <div size_full>
                    <div size=40.0 on_click={move |_| deleted.set(100)}/>
                    <alert open=open title="Delete 2 conversations?" test_id="confirm"
                         message="This can't be undone."
                         action={row_action("Delete", move || deleted.update(|count| *count += 1)).destructive()}
                         cancel="Cancel"/>
                </div>
            }
        });
    (ui, open, deleted)
}
