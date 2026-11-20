use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{InputKind, Styled, div, text};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::UNIT;
use rouge_widgets::{IconName, icon_tile, list_group, list_row, muted_text, toggle};

#[test]
fn a_group_is_a_titled_card_of_touch_rows_that_stays_on_the_grid() -> TestResult {
    let _entered = Runtime::new().enter();
    let (alerts, opened) = (signal(true), signal(0));
    let mut ui = component_test()
        .size(390.0, 400.0)
        .input(InputKind::Touch)
        .build(move || {
            view! {
                <div flex col p=16.0 size_full>
                    <list_group title="Device" test_id="device">
                        <list_row test_id="wifi" leading={icon_tile(IconName::Cloud)} chevron
                             trailing={muted_text("Studio 5G".to_owned())}
                             on_click={move || opened.update(|count| *count += 1)}>
                            <text>"Wi-Fi"</text>
                        </list_row>
                        <list_row test_id="alerts" trailing={toggle(alerts).test_id("switch")}>
                            <text>"Notifications"</text>
                        </list_row>
                        <list_row test_id="passcode" detail="Required after 5 minutes" chevron>
                            <text>"Passcode"</text>
                        </list_row>
                    </list_group>
                </div>
            }
        });
    ui.run(0.5);
    let (group, wifi, alerts_row, passcode) = (
        ui.rect_of("device")?,
        ui.rect_of("wifi")?,
        ui.rect_of("alerts")?,
        ui.rect_of("passcode")?,
    );
    assert_eq!((wifi.size.height, alerts_row.size.height), (44.0, 44.0));
    assert_eq!(
        passcode.size.height, 56.0,
        "a detail line makes a taller row"
    );
    assert_eq!(
        alerts_row.origin.y,
        wifi.max().y,
        "the rules between rows take no room"
    );
    assert_eq!(
        group.size.height % UNIT,
        0.0,
        "so the card stays on the grid"
    );
    assert_eq!((wifi.origin.x, wifi.size.width), (16.0, 358.0));
    let (title, label) = (
        ui.text_bounds("Device").ok_or("title")?,
        ui.text_bounds("Wi-Fi").ok_or("label")?,
    );
    assert!(title.max().y <= wifi.origin.y);
    assert!(
        label.origin.x >= wifi.origin.x + 12.0 + 28.0,
        "after the tile"
    );
    let (value, detail) = (
        ui.text_bounds("Studio 5G").ok_or("value")?,
        ui.text_bounds("Required after 5 minutes").ok_or("detail")?,
    );
    assert!(
        value.max().x < wifi.max().x - 12.0,
        "the chevron follows the value"
    );
    assert!(detail.origin.y > ui.text_bounds("Passcode").ok_or("passcode")?.origin.y);
    ui.tap_on("wifi")?;
    assert_eq!(opened.get(), 1);
    ui.tap_on("switch")?;
    assert!(!alerts.get(), "a control in the trailing slot still works");
    Ok(())
}

#[test]
fn outside_a_group_a_row_gains_the_same_slots_without_changing_its_height() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(300.0, 200.0).build(|| {
        view! {
            <div flex col size_full>
                <list_row test_id="plain"><text>"Plain"</text></list_row>
                <list_row test_id="chevron" chevron><text>"More"</text></list_row>
                <list_row test_id="detail" detail="Second line"><text>"Two lines"</text></list_row>
            </div>
        }
    });
    ui.run(0.5);
    assert_eq!(
        ui.rect_of("plain")?.size.height,
        ui.rect_of("chevron")?.size.height
    );
    assert_eq!(ui.rect_of("detail")?.size.height, 40.0);
    Ok(())
}
