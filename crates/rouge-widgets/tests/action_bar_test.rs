mod common;

use common::handheld_test;
use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{Styled, div, show};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{ButtonVariant, IconName, action_bar, button, row_action, tabs};

#[test]
fn with_a_pointer_the_bar_is_a_strip_of_buttons_at_the_trailing_edge() -> TestResult {
    let _entered = Runtime::new().enter();
    let archived = signal(0);
    let mut ui = component_test().size(600.0, 200.0).build(move || {
        view! {
            <div flex col size_full>
                <action_bar test_id="bar"
                     action={row_action("Archive", move || archived.update(|count| *count += 1)).icon(IconName::Archive).test_id("archive")}>
                    <button label="Save".to_owned() test_id="save" variant=ButtonVariant::Primary/>
                </action_bar>
            </div>
        }
    });
    let (bar, archive, save) = (
        ui.rect_of("bar")?,
        ui.rect_of("archive")?,
        ui.rect_of("save")?,
    );
    assert_eq!(
        (bar.origin.x, bar.size.width, bar.size.height),
        (0.0, 600.0, 40.0)
    );
    assert!(archive.max().x <= save.origin.x && save.max().x <= 600.0 - 12.0 + 0.5);
    assert!(
        ui.text_bounds("Archive").is_some(),
        "an action is a labelled button"
    );
    ui.click_on("archive")?;
    assert_eq!(archived.get(), 1);
    Ok(())
}

#[test]
fn on_a_handheld_icon_actions_float_in_the_tab_bars_slot_and_take_its_place() -> TestResult {
    let _entered = Runtime::new().enter();
    let (tab, selecting, deleted) = (signal(0usize), signal(false), signal(0));
    let mut ui = handheld_test().build(move || {
        let inbox = move || {
            view! {
                <div relative size_full>
                    <show when={move || selecting.get()}>
                        <action_bar test_id="actions"
                             action={row_action("Archive", || {}).icon(IconName::Archive).test_id("archive")}
                             action={row_action("Delete", move || deleted.update(|count| *count += 1)).icon(IconName::X)
                                 .destructive()
                                 .test_id("delete")}/>
                    </show>
                </div>
            }
        };
        view! {
            <tabs selected=tab test_id="tabs" bar tab=("Inbox", inbox) tab=("You", || view! { <div/> }) size_full/>
        }
    });
    ui.run(1.0);
    let tabs_bar = ui.rect_of("tabs-bar")?;
    selecting.set(true);
    ui.run(1.0);
    assert_eq!(ui.bounds_of("tabs-bar"), None, "the tab bar gives way");
    let (bar, archive, delete) = (
        ui.rect_of("actions")?,
        ui.rect_of("archive")?,
        ui.rect_of("delete")?,
    );
    assert_eq!(
        (bar.origin.x, bar.max().y, bar.size.width),
        (tabs_bar.origin.x, tabs_bar.max().y, tabs_bar.size.width),
        "in the same slot"
    );
    assert_eq!(archive.size, delete.size, "actions share the width");
    assert_eq!(archive.size.height, 44.0);
    ui.tap_on("delete")?;
    assert_eq!(deleted.get(), 1);
    selecting.set(false);
    ui.run(1.0);
    assert!(
        ui.bounds_of("tabs-bar").is_some(),
        "and comes back afterwards"
    );
    Ok(())
}

#[test]
fn on_a_handheld_buttons_share_a_footer_that_rides_above_the_keyboard() -> TestResult {
    let _entered = Runtime::new().enter();
    let published = signal(false);
    let mut ui = handheld_test().build(move || {
        view! {
            <div relative size_full>
                <action_bar test_id="footer">
                    <button label="Save draft".to_owned() test_id="draft" variant=ButtonVariant::Secondary/>
                    <button label="Publish".to_owned() test_id="publish" variant=ButtonVariant::Primary
                         on_click={move || published.set(true)}/>
                </action_bar>
            </div>
        }
    });
    ui.run(0.5);
    let (footer, draft, publish) = (
        ui.rect_of("footer")?,
        ui.rect_of("draft")?,
        ui.rect_of("publish")?,
    );
    assert_eq!(
        (footer.origin.x, footer.max().x, footer.max().y),
        (12.0, 378.0, 688.0)
    );
    assert_eq!(draft.size, publish.size, "the buttons share the width");
    assert_eq!(draft.size.height, 44.0);
    assert!(draft.size.width > 150.0, "and fill it: {draft:?}");
    ui.set_keyboard_inset(300.0);
    ui.run(0.5);
    assert_eq!(ui.rect_of("footer")?.max().y, 700.0 - 300.0 - 12.0);
    ui.tap_on("publish")?;
    assert!(published.get());
    Ok(())
}

#[test]
fn two_bars_on_one_handheld_window_settle_instead_of_chasing_each_other() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = handheld_test().build(|| {
        let bar = |id: &'static str| {
            view! {
                <action_bar test_id=id>
                    <button label="Save".to_owned() variant=ButtonVariant::Primary/>
                </action_bar>
            }
        };
        view! {
            <div relative size_full>
                {bar("first")}
                {bar("second")}
            </div>
        }
    });
    ui.run(0.5);
    assert_eq!(ui.rect_of("first")?, ui.rect_of("second")?);
    Ok(())
}
