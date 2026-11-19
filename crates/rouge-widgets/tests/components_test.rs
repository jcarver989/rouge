use rouge_core::testing::{TestError, TestResult, TestUi, assert_near, counter};
use rouge_core::{
    Color, Cursor, Div, InputKind, Key, KeyEvent, Point, PointerButton, Rect, Styled, Theme,
    UiPalette, View, div, show, text, view,
};
use rouge_reactive::{Runtime, Signal, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::{Hold, ICON_BUTTON_SIZE, UNIT};
use rouge_widgets::*;

#[test]
fn full_width_buttons_rows_and_segments_activate_only_enabled_choices() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, calls) = component_test().size(440.0, 320.0).build_with(|| {
        let calls = signal(Vec::new());
        let view = view! {
            <div flex col p=12.0 w=340.0 material=Material::Glass>
                <button test_id="primary" label="Primary" variant=ButtonVariant::Primary w_full
                        on_click={move || calls.update(|v| v.push(1))}/>
                <list_row test_id="row" depth=2 selection=SelectionState::Selected
                          on_click={move || calls.update(|v| v.push(2))}>
                    <text>"A row"</text>
                </list_row>
                <segmented_control test_id="segments">
                    <segment test_id="enabled" label="Enabled" selected=true on_click={move || calls.update(|v| v.push(3))}/>
                    <segment test_id="disabled" label="Disabled" icon=IconName::Check detail="Extra" disabled=true
                             on_click={move || calls.update(|v| v.push(4))}/>
                </segmented_control>
            </div>
        };
        (view, calls)
    });
    let button = ui.rect_of("primary")?;
    assert!(button.size.width > 300.0);
    ui.click_on("primary")?;
    ui.click_on("row")?;
    let detail = ui.center_of("disabled-detail")?;
    let disabled = ui.rect_of("disabled")?;
    assert!(disabled.contains(detail));
    ui.click_on("disabled")?;
    ui.click_on("enabled")?;
    ui.press("space");
    assert_eq!(calls.get(), vec![1, 2, 3, 3]);
    Ok(())
}

#[test]
fn section_action_and_disclosure_do_not_trigger_each_other() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, counts) = component_test().size(440.0, 320.0).build_with(|| {
        let counts = signal((0, 0));
        let badge = view! { <div test_id="badge"><text caption>"3"</text></div> };
        let action = view! {
            <icon_button test_id="action" icon=IconName::Plus label="Add" on_click={move || counts.update(|v| v.0 += 1)}/>
        };
        let view = view! {
            <div>
                <section_header test_id="section" title="Section" expanded=true detail="Details" badge=badge.into_any()
                                action=action actions_always_visible on_toggle={move || counts.update(|v| v.1 += 1)}/>
                <empty_state icon=IconName::File title="No files" detail="Nothing to review"/>
                <action_bar><notification message="Not saved" tone=NoticeTone::Error/></action_bar>
            </div>
        };
        (view, counts)
    });
    ui.click_on("action")?;
    assert_eq!(counts.get(), (1, 0));
    ui.click_on("section-toggle")?;
    assert_eq!(counts.get(), (1, 1));
    assert!(ui.bounds_of("badge").is_some());
    Ok(())
}

#[test]
fn checked_styles_apply_while_a_checkbox_is_checked() -> TestResult {
    let _entered = Runtime::new().enter();
    let checked = signal(false);
    let mut ui = component_test().size(120.0, 120.0).build(move || {
        view! { <checkbox test_id="check" checked size=12.0 checked:size=20.0/> }
    });
    assert_eq!(ui.rect_of("check")?.size.width, 12.0);
    ui.click_on("check")?;
    assert!(checked.get());
    assert_eq!(ui.rect_of("check")?.size.width, 20.0);
    Ok(())
}

#[test]
fn an_icon_button_paints_its_icon_in_the_given_color() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(64.0, 64.0).build(|| {
        view! {
            <icon_button icon=IconName::Plus label="Add" color=Color::RED size=64.0 icon_size=64.0/>
        }
    });
    ui.advance(0.0);
    let [red, green, blue, _] = ui.pixel(32, 32)?;
    assert!(
        red > 200 && green < 60 && blue < 60,
        "got {red} {green} {blue}"
    );
    Ok(())
}

#[test]
fn the_segment_indicator_is_an_elevated_fill_on_the_chrome_track() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(320.0, 120.0).build(move || {
        view! {
            <segmented_control test_id="segments">
                <segment test_id="first" label="First" w=80.0 selected=true/>
                <segment test_id="second" label="Second" w=80.0/>
            </segmented_control>
        }
    });
    ui.run(0.5);
    let light = UiPalette::light().map(Color::from);
    let (first, second) = (ui.rect_of("first")?, ui.rect_of("second")?);
    let y = first.center().y as u32;
    assert_near(
        ui.pixel(first.origin.x as u32 + 6, y)?,
        light.elevated.to_rgba8(),
        "the selected segment sits on an elevated fill",
    );
    assert_near(
        ui.pixel(second.origin.x as u32 + 6, y)?,
        light.chrome.to_rgba8(),
        "the other segments show the track",
    );
    Ok(())
}

#[test]
fn a_labelled_segment_shows_its_label_beside_its_icon_and_holds_a_badge_in_its_corner() -> TestResult
{
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(320.0, 120.0).build(|| {
        let dot = view! { <div test_id="dot" size=8.0 bg=Color::RED/> };
        view! {
            <segmented_control test_id="segments">
                <segment test_id="files" label="Files" icon=IconName::Folder detail="3" labelled badge=dot selected=true/>
            </segmented_control>
        }
    });
    ui.run(0.5);
    let (files, icon, dot) = (
        ui.rect_of("files")?,
        ui.rect_of("files-icon")?,
        ui.rect_of("dot")?,
    );
    let label = ui.text_bounds("Files").ok_or("the label is shown")?;
    let detail = ui.rect_of("files-detail")?;
    assert!(
        icon.max().x <= label.origin.x && label.max().x <= detail.origin.x,
        "{icon:?} {label:?} {detail:?}"
    );
    assert_eq!((dot.origin.y, dot.max().x), (files.origin.y, files.max().x));
    assert_eq!(files.size.height % UNIT, 0.0);
    Ok(())
}

#[test]
fn the_segment_indicator_hides_without_a_selection_and_returns_with_one() -> TestResult {
    let _entered = Runtime::new().enter();
    let selected = signal(Some(0));
    let mut ui = component_test().size(320.0, 120.0).build(move || {
        view! {
            <segmented_control test_id="segments" indicator:bg=Color::RED>
                <segment test_id="first" label="First" w=80.0 selected={move || selected.get() == Some(0)}/>
                <segment test_id="second" label="Second" w=80.0 selected={move || selected.get() == Some(1)}/>
            </segmented_control>
        }
    });
    ui.run(0.5);
    let first = ui.rect_of("first")?;
    let (x, y) = (first.origin.x as u32 + 6, first.center().y as u32);
    assert_eq!(ui.rect_of("segments-indicator")?, first);
    assert!(is_red(ui.pixel(x, y)?));
    selected.set(None);
    ui.run(0.5);
    assert!(!is_red(ui.pixel(x, y)?));
    selected.set(Some(1));
    ui.run(1.0);
    assert_eq!(ui.rect_of("segments-indicator")?, ui.rect_of("second")?);
    assert!(!is_red(ui.pixel(x, y)?));
    Ok(())
}

#[test]
fn checkboxes_keep_their_size_and_toggle_without_activating_their_row() -> TestResult {
    let _entered = Runtime::new().enter();
    let (state, plain, disabled, rows) = (
        signal(CheckState::Indeterminate),
        signal(false),
        signal(true),
        signal(0),
    );
    let mut ui = component_test().size(440.0, 320.0).build(move || {
        view! {
            <list_row test_id="row" on_click={move || rows.update(|count| *count += 1)}>
                <text>"file.rs"</text>
                <checkbox test_id="check" checked=state/>
                <checkbox test_id="disabled" checked=plain disabled=disabled/>
            </list_row>
        }
    });
    let bounds = ui.rect_of("check")?;
    assert_eq!((bounds.size.width, bounds.size.height), (16.0, 16.0));
    ui.click_on("check")?;
    assert_eq!((state.get(), rows.get()), (CheckState::Checked, 0));
    ui.press("space");
    assert_eq!(state.get(), CheckState::Unchecked);
    ui.click_on("disabled")?;
    assert_eq!(
        (plain.get(), rows.get()),
        (false, 1),
        "the click falls through to the row"
    );
    disabled.set(false);
    ui.advance(0.0);
    ui.click_on("disabled")?;
    assert_eq!((plain.get(), rows.get()), (true, 1));
    Ok(())
}

#[test]
fn an_icon_button_shrinks_while_pressed_and_takes_an_accent_focus_border() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test()
        .size(120.0, 120.0)
        .clear(Color::WHITE)
        .build(|| {
            view! {
                <div p=40.0 flex>
                    <icon_button test_id="copy" icon=IconName::Copy label="Copy" selected=true on_click={|| {}}/>
                </div>
            }
        });
    let at = ui.rect_of("copy")?;
    let (x, y) = (at.origin.x as u32 + 1, (at.origin.y + 12.0) as u32);
    let rest = ui.pixel(x - 1, y)?;
    assert_ne!(
        ui.pixel(x, y)?,
        [255; 4],
        "a selected icon button is filled"
    );
    let center = ui.center_of("copy")?;
    ui.pointer_moved(center);
    ui.pointer_down(PointerButton::Primary);
    ui.run(0.5);
    let pressed = ui.pixel(x - 1, y)?;
    assert!(
        pressed[0] > rest[0],
        "the pressed button pulls in from its edge: {rest:?} then {pressed:?}"
    );
    ui.pointer_up(PointerButton::Primary);
    ui.pointer_moved(Point::new(5.0, 5.0));
    ui.run(0.5);
    assert_eq!(ui.pixel(x - 1, y)?, rest, "and springs back on release");
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    ui.press("tab");
    assert!(ui.has_focus());
    ui.run(0.5);
    let accent = UiPalette::light().map(Color::from).accent.to_rgba8();
    assert_near(ui.pixel(x - 1, y)?, accent, "focus border");
    Ok(())
}

#[test]
fn a_checkbox_mark_grows_in_and_then_idles() -> TestResult {
    let _entered = Runtime::new().enter();
    let checked = signal(false);
    let mut ui = component_test().size(120.0, 120.0).build(move || {
        view! { <div p=20.0 flex><checkbox test_id="check" checked/></div> }
    });
    let created = ui.stats().elements_created;
    ui.click_on("check")?;
    assert!(checked.get());
    ui.advance(0.0);
    assert!(ui.is_animating(), "the tick animates in");
    ui.run(1.0);
    assert_eq!(ui.compositor_frames(1.0)?, 0);
    assert_eq!(ui.stats().elements_created, created, "no remount");
    Ok(())
}

#[test]
fn a_disclosure_reveals_its_body_and_an_accordion_keeps_one_open() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(Some(0usize));
    let mut ui = component_test().size(300.0, 300.0).build(move || {
        let item = move |key: usize, name: &'static str| {
            view! {
                <disclosure expanded=false accordion=(open, key) test_id=name
                     child={view! { <text>{name}</text> }}
                     body={move || view! { <div test_id={format!("{name}-body")} h=40.0/> }}/>
            }
        };
        view! {
            <div flex col p=10.0>
                {item(0, "playback")}
                {item(1, "downloads")}
            </div>
        }
    });
    ui.run(0.5);
    assert!(ui.bounds_of("playback-body").is_some());
    assert!(ui.bounds_of("downloads-body").is_none());
    let before = ui.rect_of("downloads")?;
    ui.click_on("downloads")?;
    assert_eq!(open.get(), Some(1), "opening one closes the other");
    ui.advance(0.0);
    assert!(ui.is_animating(), "the rows below slide");
    ui.run(1.0);
    assert!(ui.bounds_of("playback-body").is_none());
    assert!(ui.bounds_of("downloads-body").is_some());
    assert_eq!(ui.rect_of("downloads")?.origin.y, before.origin.y - 40.0);
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    ui.press("tab");
    ui.press("tab");
    ui.press("enter");
    assert_eq!(open.get(), None, "the keyboard toggles the focused header");
    Ok(())
}

#[test]
fn empty_states_and_notifications_take_an_action() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, calls) = component_test().size(440.0, 320.0).build_with(|| {
        let calls = signal(Vec::new());
        let browse = view! {
            <button label="Browse library" test_id="browse"
                 on_click={move || calls.update(|calls| calls.push("browse"))}/>
        };
        let retry = view! {
            <button label="Retry" test_id="retry"
                 on_click={move || calls.update(|calls| calls.push("retry"))}/>
        };
        let root = view! {
            <div flex col gap=8.0>
                <empty_state icon=IconName::File title="No files".to_owned()
                     detail="Nothing to review".to_owned() test_id="empty">
                    {browse}
                </empty_state>
                <notification message="Could not reach the speaker".to_owned() tone=NoticeTone::Error
                     title?=Some("Playback failed".to_owned()) test_id="notice">
                    {retry}
                </notification>
            </div>
        };
        (root, calls)
    });
    let empty = ui.rect_of("empty")?;
    assert!(empty.contains(ui.center_of("browse")?));
    let notice = ui.rect_of("notice")?;
    assert!(notice.contains(ui.center_of("retry")?));
    let title = ui
        .text_bounds("Playback failed")
        .ok_or("the title is drawn")?;
    let body = ui
        .text_bounds("Could not reach the speaker")
        .ok_or("the body is drawn")?;
    assert!(
        title.max().y <= body.origin.y,
        "the title sits above the body"
    );
    ui.click_on("browse")?;
    ui.click_on("retry")?;
    assert_eq!(calls.get(), ["browse", "retry"]);
    Ok(())
}

#[test]
fn row_actions_appear_on_hover_and_on_keyboard_focus_with_a_pointer() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, log) = component_test().size(300.0, 120.0).build_with(action_row);
    ui.run(1.0);
    let row = ui.rect_of("row")?;
    let archive = ui.rect_of("archive")?;
    assert!(
        archive.max().x <= row.max().x && archive.origin.x > row.center().x,
        "actions sit at the trailing edge: {archive:?} in {row:?}"
    );
    let inked = |ui: &mut TestUi| -> Result<bool, TestError> {
        let (y, page) = (archive.center().y as u32, ui.pixel(2, 2)?);
        let (from, to) = (archive.origin.x as u32, archive.max().x as u32);
        for x in from..to {
            if ui.pixel(x, y)? != page {
                return Ok(true);
            }
        }
        Ok(false)
    };
    assert!(!inked(&mut ui)?, "hidden until the row is hovered");
    ui.hover_on("row")?;
    ui.run(1.0);
    assert!(inked(&mut ui)?, "hovering reveals them");
    ui.click_on("archive")?;
    assert_eq!(log.get(), ["archive"], "an action does not click its row");
    ui.click_at(20.0, row.center().y);
    assert_eq!(log.get(), ["archive", "row"]);
    ui.pointer_left();
    ui.clear_focus();
    ui.run(1.0);
    ui.press("tab");
    ui.press("tab");
    ui.press("tab");
    ui.press("enter");
    assert_eq!(
        log.get(),
        ["archive", "row", "delete"],
        "tab reaches each action"
    );
    Ok(())
}

#[test]
fn an_icon_only_row_action_is_a_square_chip_named_by_its_tooltip_but_a_labelled_block_on_touch()
-> TestResult {
    for (input, labelled) in [(InputKind::Pointer, false), (InputKind::Touch, true)] {
        let _entered = Runtime::new().enter();
        let (mut ui, log) = component_test()
            .size(300.0, 120.0)
            .input(input)
            .build_with(icon_only_row);
        ui.run(1.0);
        let archive = ui.rect_of("archive")?;
        assert_eq!(ui.text_bounds("Archive").is_some(), labelled, "{input:?}");
        if !labelled {
            assert_eq!(
                (archive.size.width, archive.size.height),
                (ICON_BUTTON_SIZE, ICON_BUTTON_SIZE)
            );
            ui.hover_on("row")?;
            ui.click_on("archive")?;
            assert_eq!(log.get(), ["archive"]);
        }
    }
    Ok(())
}

#[test]
fn a_trailing_label_keeps_the_row_edge_and_gives_way_to_the_actions_on_hover() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, log) = component_test().size(300.0, 120.0).build_with(aged_row);
    ui.run(1.0);
    let row = ui.rect_of("row")?;
    let age = ui.rect_of("age")?;
    let (archive, delete) = (ui.rect_of("archive")?, ui.rect_of("delete")?);
    assert_eq!(age.max().x, delete.max().x, "both end at the trailing edge");
    assert!(age.max().x <= row.max().x, "{age:?} in {row:?}");
    assert!(
        archive.origin.x < age.origin.x,
        "the actions lie over the row instead of reserving room beside the label"
    );
    let page = ui.pixel(2, 2)?;
    let inked = |ui: &mut TestUi, rect: Rect| -> Result<bool, TestError> {
        let y = rect.center().y as u32;
        for x in rect.origin.x as u32..rect.max().x as u32 {
            if ui.pixel(x, y)? != page {
                return Ok(true);
            }
        }
        Ok(false)
    };
    assert!(inked(&mut ui, age)?, "the label shows at rest");
    assert!(!inked(&mut ui, archive)?, "the actions are hidden at rest");
    ui.hover_on("row")?;
    ui.run(1.0);
    ui.click_on("archive")?;
    assert_eq!(log.get(), ["archive"]);
    Ok(())
}

#[test]
fn row_actions_are_revealed_by_a_swipe_on_touch() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, log) = component_test()
        .size(300.0, 120.0)
        .input(InputKind::Touch)
        .build_with(action_row);
    ui.run(1.0);
    let row = ui.rect_of("row")?;
    assert_eq!(row.size.height, 44.0, "rows are touch sized");
    assert!(
        ui.rect_of("delete")?.origin.x >= row.max().x,
        "actions wait past the trailing edge"
    );
    let (y, right) = (row.center().y, row.max().x);
    ui.swipe_over(
        Point::new(right - 20.0, y),
        Point::new(right - 50.0, y),
        0.5,
    );
    ui.run(2.0);
    assert_eq!(
        ui.rect_of("row")?.origin.x,
        row.origin.x,
        "a short swipe springs back"
    );
    ui.swipe_over(
        Point::new(right - 20.0, y),
        Point::new(right - 200.0, y),
        0.5,
    );
    ui.run(2.0);
    let delete = ui.rect_of("delete")?;
    assert_eq!(
        delete.max().x,
        right,
        "a long swipe rests with the actions in view"
    );
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    ui.tap(Point::new(row.origin.x + 12.0, y));
    ui.run(2.0);
    assert_eq!(
        (ui.rect_of("row")?.origin.x, log.get().len()),
        (row.origin.x, 0),
        "a tap on an open row closes it instead of clicking"
    );
    ui.swipe_over(
        Point::new(right - 20.0, y),
        Point::new(right - 200.0, y),
        0.5,
    );
    ui.run(2.0);
    ui.tap_on("delete")?;
    ui.run(2.0);
    assert_eq!(log.get(), ["delete"]);
    assert_eq!(
        ui.rect_of("row")?.origin.x,
        row.origin.x,
        "choosing closes the row"
    );
    Ok(())
}

#[test]
fn a_button_fires_on_click_and_on_enter_or_space_when_focused() -> TestResult {
    let _entered = Runtime::new().enter();
    let (clicks, click) = counter();
    let mut ui = component_test().build(move || {
        view! {
            <div p=10.0 flex>{view! { <button test_id="save" label="Save" on_click=click/> }}</div>
        }
    });
    let center = ui.center_of("save")?;
    ui.pointer_moved(center);
    assert_eq!(ui.cursor(), Cursor::Pointer);
    ui.click_on("save")?;
    assert_eq!(clicks.get(), 1);
    assert!(ui.press("enter"));
    assert!(ui.press("space"));
    assert!(!ui.press("escape"), "other keys are left alone");
    assert_eq!(clicks.get(), 3);
    Ok(())
}

#[test]
fn a_disabled_button_ignores_input_and_cannot_be_focused() -> TestResult {
    let _entered = Runtime::new().enter();
    let disabled = signal(true);
    let (clicks, click) = counter();
    let mut ui = component_test().build(move || {
        view! {
            <div p=10.0 flex>
                <button label="Save" test_id="save" disabled=disabled on_click=click/>
            </div>
        }
    });
    let created = ui.stats().elements_created;
    ui.click_on("save")?;
    ui.press("tab");
    assert!(!ui.has_focus());
    ui.press("enter");
    assert_eq!(clicks.get(), 0);
    disabled.set(false);
    ui.advance(0.0);
    ui.press("tab");
    ui.press("enter");
    ui.click_on("save")?;
    assert_eq!(clicks.get(), 2);
    assert_eq!(
        ui.stats().elements_created,
        created,
        "enabling does not remount"
    );
    Ok(())
}

#[test]
fn a_pressed_button_shrinks_until_the_pointer_is_released_even_outside_it() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = wide(ButtonVariant::Primary);
    let accent = UiPalette::light().map(Color::from).accent.to_rgba8();
    let y = edge_row(ui.rect_of("save")?);
    let rest = ui.first_x(accent, y)?;
    let center = ui.center_of("save")?;
    ui.pointer_moved(center);
    ui.pointer_down(PointerButton::Primary);
    ui.run(0.5);
    let pressed = ui.first_x(accent, y)?;
    assert!(pressed >= rest + 3, "pressed edge {pressed} vs rest {rest}");
    ui.pointer_moved(center + Point::new(0.0, 60.0));
    ui.pointer_up(PointerButton::Primary);
    ui.run(0.5);
    assert_eq!(ui.first_x(accent, y)?, rest);
    Ok(())
}

#[test]
fn a_button_follows_the_theme_without_remounting() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, theme) = component_test().size(300.0, 80.0).build_with(|| {
        let root = view! {
            <div p=20.0 flex>
                <button label="Save" test_id="save" variant=ButtonVariant::Primary/>
            </div>
        };
        (root, Theme::current())
    });
    let at = ui.rect_of("save")?;
    let (x, y) = (at.origin.x as u32 + 4, edge_row(at));
    let created = ui.stats().elements_created;
    let palette = |palette: UiPalette| palette.map(Color::from).accent.to_rgba8();
    assert_near(ui.pixel(x, y)?, palette(UiPalette::light()), "light accent");
    theme.set(UiPalette::dark());
    ui.run(1.0);
    assert_near(ui.pixel(x, y)?, palette(UiPalette::dark()), "dark accent");
    assert_eq!(ui.stats().elements_created, created, "no remount");
    Ok(())
}

#[test]
fn secondary_and_destructive_buttons_draw_a_hairline_edge_on_a_flat_fill() -> TestResult {
    let _entered = Runtime::new().enter();
    let light = UiPalette::light().map(Color::from);
    let mut ui = wide(ButtonVariant::Secondary);
    let at = ui.rect_of("save")?;
    let (x, y) = (at.origin.x as u32, edge_row(at));
    assert_near(ui.pixel(x, y)?, light.border.to_rgba8(), "hairline edge");
    assert_near(ui.pixel(x + 3, y)?, light.elevated.to_rgba8(), "flat fill");
    let mut ui = wide(ButtonVariant::Destructive);
    let [red, green, blue, _] = ui.pixel(x, y)?;
    assert!(
        red > green.saturating_add(40) && red > blue.saturating_add(40),
        "destructive edge is tinted, got {red} {green} {blue}"
    );
    assert_near(ui.pixel(x + 3, y)?, [255; 4], "destructive has no fill");
    Ok(())
}

#[test]
fn a_loading_button_keeps_its_width_shows_a_spinner_and_ignores_input() -> TestResult {
    let _entered = Runtime::new().enter();
    let loading = signal(false);
    let (clicks, click) = counter();
    let mut ui = component_test().build(move || {
        view! {
            <div p=10.0 flex>
                <button label="Publish" test_id="publish" variant=ButtonVariant::Primary loading=loading
                     on_click=click/>
            </div>
        }
    });
    let rest = ui.rect_of("publish")?;
    assert!(ui.bounds_of("publish-spinner").is_none());
    assert_eq!(
        ui.compositor_frames(1.0)?,
        0,
        "an idle button draws nothing"
    );
    loading.set(true);
    ui.run(0.3);
    assert_eq!(ui.rect_of("publish")?, rest, "the width is kept");
    assert!(rest.contains(ui.center_of("publish-spinner")?));
    assert!(ui.is_animating(), "the spinner turns");
    ui.click_on("publish")?;
    ui.press("tab");
    ui.press("enter");
    assert_eq!(clicks.get(), 0, "it cannot be activated while loading");
    loading.set(false);
    ui.run(0.3);
    assert!(ui.bounds_of("publish-spinner").is_none());
    ui.click_on("publish")?;
    assert_eq!(clicks.get(), 1);
    Ok(())
}

#[test]
fn a_confirm_button_fires_only_after_being_held_and_springs_back_when_released_early() -> TestResult
{
    let _entered = Runtime::new().enter();
    let (clicks, click) = counter();
    let mut ui = component_test()
        .size(300.0, 80.0)
        .clear(Color::WHITE)
        .build(move || {
            view! {
                <div p=20.0 flex>
                    <button label="Delete library" test_id="delete" variant=ButtonVariant::Destructive
                         confirm=Hold::seconds(1.0) on_click=click w=200.0/>
                </div>
            }
        });
    let at = ui.rect_of("delete")?;
    let (left, right, y) = (
        at.origin.x as u32 + 20,
        at.max().x as u32 - 20,
        at.origin.y as u32 + 4,
    );
    ui.pointer_moved(at.center());
    ui.run(0.5);
    let rest = ui.pixel(left, y)?;
    ui.click_on("delete")?;
    ui.run(0.5);
    assert_eq!(clicks.get(), 0, "a click is not enough");
    assert_eq!(ui.pixel(left, y)?, rest, "and the fill springs back");
    ui.pointer_down(PointerButton::Primary);
    ui.run(0.5);
    assert_ne!(
        ui.pixel(left, y)?,
        rest,
        "the fill grows from the left while held"
    );
    assert_eq!(
        ui.pixel(right, y)?,
        rest,
        "and has not reached the far side"
    );
    assert_eq!(clicks.get(), 0);
    ui.run(0.7);
    assert_eq!(clicks.get(), 1, "holding to the end fires once");
    ui.pointer_up(PointerButton::Primary);
    ui.run(0.5);
    assert_eq!(ui.pixel(left, y)?, rest);
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    ui.press("tab");
    ui.key_down(KeyEvent::new(Key::Space));
    ui.run(0.4);
    ui.key_up(KeyEvent::new(Key::Space));
    ui.run(0.5);
    assert_eq!(clicks.get(), 1, "releasing the key early cancels");
    ui.key_down(KeyEvent::new(Key::Enter));
    ui.run(1.2);
    ui.key_up(KeyEvent::new(Key::Enter));
    assert_eq!(clicks.get(), 2, "holding a key confirms");
    Ok(())
}

#[test]
fn a_confirm_icon_button_fires_only_after_being_held_by_pointer_or_key() -> TestResult {
    let _entered = Runtime::new().enter();
    let (clicks, click) = counter();
    let mut ui = component_test().size(120.0, 80.0).build(move || {
        view! {
            <div p=20.0 flex>
                <icon_button test_id="archive" icon=IconName::Archive label="Archive" confirm=Hold::seconds(1.0)
                             on_click=click/>
            </div>
        }
    });
    ui.click_on("archive")?;
    ui.run(1.5);
    assert_eq!(clicks.get(), 0, "a click is not enough");
    let at = ui.center_of("archive")?;
    ui.pointer_moved(at);
    ui.pointer_down(PointerButton::Primary);
    ui.run(1.2);
    assert_eq!(clicks.get(), 1, "holding to the end fires once");
    ui.pointer_up(PointerButton::Primary);
    ui.run(0.5);
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    ui.key_down(KeyEvent::new(Key::Enter));
    ui.run(1.2);
    ui.key_up(KeyEvent::new(Key::Enter));
    assert_eq!(clicks.get(), 2, "holding a key confirms");
    Ok(())
}

#[test]
fn a_button_takes_the_touch_size_from_the_presentation_unless_sized_explicitly() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().build(|| {
        view! {
            <div p=10.0 flex gap=8.0>
                <button label="Save" test_id="save"/>
                <button label="Small" test_id="small" control_size=ControlSize::Small/>
                <button label="Play" test_id="play" control_size=ControlSize::Large/>
            </div>
        }
    });
    let created = ui.stats().elements_created;
    assert_eq!(ui.rect_of("save")?.size.height, 28.0, "with a pointer");
    assert_eq!(ui.rect_of("play")?.size.height, 44.0, "a touch target");
    ui.set_input(InputKind::Touch);
    ui.advance(0.0);
    assert_eq!(ui.rect_of("save")?.size.height, 44.0, "on touch");
    assert_eq!(
        ui.rect_of("small")?.size.height,
        24.0,
        "an explicit size is kept"
    );
    assert_eq!(
        ui.stats().elements_created,
        created,
        "the size follows the presentation without a remount"
    );
    Ok(())
}

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
                <modal test_id="dialog" title="Confirm" on_dismiss={move || open.set(false)}>
                    <div flex gap=8.0>
                        <button test_id="cancel" label="Cancel" on_click={move || open.set(false)}/>
                        <button test_id="ok" label="OK" on_click=confirm/>
                    </div>
                </modal>
            }
        };
        view! {
            <div size_full>
                <div absolute left=0.0 top=350.0 size=40.0 on_click={move |_| hit_behind()}/>
                <show when={move || open.get()} children=dialog/>
            </div>
        }
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

#[test]
fn a_modal_scales_and_fades_in_then_idles() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(false);
    let mut ui = component_test().size(600.0, 400.0).build(move || {
        let dialog =
            move || view! { <modal test_id="dialog" title="Confirm"><div h=40.0/></modal> };
        view! {
            <div size_full>
                <show when={move || open.get()} children=dialog/>
            </div>
        }
    });
    let page = ui.pixel(590, 390)?;
    open.set(true);
    ui.advance(0.0);
    assert!(ui.is_animating(), "the scrim and the panel animate in");
    ui.advance(1.0 / 60.0);
    let entering = ui.pixel(590, 390)?;
    ui.run(1.0);
    let settled = ui.pixel(590, 390)?;
    let distance = |pixel: [u8; 4]| {
        pixel
            .iter()
            .zip(page)
            .map(|(a, b)| u32::from(a.abs_diff(b)))
            .sum::<u32>()
    };
    assert!(
        distance(entering) < distance(settled),
        "the scrim fades in: {entering:?} then {settled:?}"
    );
    assert_eq!(ui.compositor_frames(1.0)?, 0, "nothing animates at rest");
    Ok(())
}

#[test]
fn a_modal_is_a_full_height_sheet_on_a_handheld() -> TestResult {
    let _entered = Runtime::new().enter();
    let open = signal(true);
    let (confirmed, confirm) = counter();
    let mut ui = component_test()
        .size(390.0, 700.0)
        .input(InputKind::Touch)
        .build(move || {
            let dialog = move || {
                let confirm = confirm.clone();
                view! {
                    <modal test_id="dialog" title="Confirm" on_dismiss={move || open.set(false)}>
                        <button test_id="ok" label="OK" on_click=confirm/>
                    </modal>
                }
            };
            view! {
                <div size_full>
                    <show when={move || open.get()} children=dialog/>
                </div>
            }
        });
    ui.run(2.0);
    let sheet = ui.rect_of("dialog")?;
    assert_eq!(
        (sheet.origin.x, sheet.origin.y, sheet.size.width),
        (0.0, 8.0, 390.0),
        "it fills the window below a small gap"
    );
    ui.tap_on("ok")?;
    assert_eq!(confirmed.get(), 1, "its content is live");
    ui.press("escape");
    assert!(open.get(), "it slides away before it reports the dismissal");
    ui.run(2.0);
    assert!(!open.get());
    assert_eq!(ui.bounds_of("dialog"), None);
    Ok(())
}

fn is_red([red, green, blue, _]: [u8; 4]) -> bool {
    red > 200 && green < 60 && blue < 60
}

fn action_row() -> (Div, Signal<Vec<&'static str>>) {
    let log = signal(Vec::new());
    let note = move |name: &'static str| move || log.update(|log| log.push(name));
    let row = view! {
        <list_row test_id="row" on_click=note("row")
             action=row_action("Archive", note("archive")).test_id("archive")
             action={row_action("Delete", note("delete"))
                 .destructive()
                 .test_id("delete")}>
            <text>"Harmattan"</text>
        </list_row>
    };
    (view! { <div size_full p=8.0>{row}</div> }, log)
}

fn aged_row() -> (Div, Signal<Vec<&'static str>>) {
    let log = signal(Vec::new());
    let note = move |name: &'static str| move || log.update(|log| log.push(name));
    let row = view! {
        <list_row test_id="row" trailing={view! { <div test_id="age"><text>"2h"</text></div> }}
             action=row_action("Archive", note("archive")).test_id("archive")
             action=row_action("Delete", note("delete")).test_id("delete")>
            <text>"Harmattan"</text>
        </list_row>
    };
    (view! { <div size_full p=8.0>{row}</div> }, log)
}

fn icon_only_row() -> (Div, Signal<Vec<&'static str>>) {
    let log = signal(Vec::new());
    let archive = move || log.update(|log| log.push("archive"));
    let row = view! {
        <list_row test_id="row" action={row_action("Archive", archive).icon(IconName::Archive).icon_only().test_id("archive")}>
            <text>"Harmattan"</text>
        </list_row>
    };
    (view! { <div size_full p=8.0>{row}</div> }, log)
}

fn wide(variant: ButtonVariant) -> TestUi {
    component_test()
        .size(300.0, 80.0)
        .clear(Color::WHITE)
        .build(move || {
            view! {
                <div p=20.0 flex>
                    <button label="Save" test_id="save" variant=variant w=200.0/>
                </div>
            }
        })
}

fn edge_row(at: Rect) -> u32 {
    (at.origin.y + 8.0) as u32
}
