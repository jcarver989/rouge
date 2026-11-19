use rouge_core::testing::{TestResult, assert_near};
use rouge_core::view;
use rouge_core::{Color, Styled, UiPalette, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::breadcrumb;
use rouge_widgets::testing::component_test;

#[test]
fn links_activate_by_pointer_and_keyboard_and_the_current_item_does_not() -> TestResult {
    let _entered = Runtime::new().enter();
    let visited = signal(Vec::new());
    let mut ui = component_test()
        .size(320.0, 60.0)
        .clear(Color::WHITE)
        .build(move || {
            let go = move |place: &'static str| move || visited.update(|seen| seen.push(place));
            view! {
                <div p=10.0>
                    <breadcrumb test_id="path" link=("Library", go("library")) link=("Playlists", go("playlists"))
                         current="Late night"/>
                </div>
            }
        });
    let (first, second) = (ui.rect_of("path-0")?, ui.rect_of("path-1")?);
    let here = ui
        .text_bounds("Late night")
        .ok_or("the current item is drawn")?;
    assert!(first.max().x < second.origin.x && second.max().x < here.origin.x);
    ui.click_on("path-1")?;
    ui.click_at(here.center().x, here.center().y);
    assert_eq!(
        visited.get(),
        ["playlists"],
        "the current item is not a link"
    );
    ui.press("tab");
    ui.press("enter");
    assert_eq!(visited.get(), ["playlists", "library"]);
    ui.run(0.3);
    let accent = UiPalette::light().map(Color::from).accent.to_rgba8();
    assert_near(
        ui.pixel(first.origin.x as u32, first.center().y as u32)?,
        accent,
        "focus border",
    );
    Ok(())
}

#[test]
fn a_long_current_item_is_clipped_to_the_room_the_breadcrumb_has() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(320.0, 60.0).build(|| {
        view! {
            <div w=160.0 flex>
                <breadcrumb test_id="path" min_w=0.0 link=("Library", || {})
                     current="A playlist with a name far too long to fit"/>
            </div>
        }
    });
    let (path, here) = (ui.rect_of("path")?, ui.rect_of("path-current")?);
    assert!(path.max().x <= 160.0, "{path:?}");
    assert!(here.max().x <= path.max().x, "{here:?} in {path:?}");
    Ok(())
}
