use rouge_core::testing::{TestResult, assert_near};
use rouge_core::view;
use rouge_core::{Color, Styled, Theme, UiPalette, div};
use rouge_reactive::Runtime;
use rouge_widgets::testing::component_test;
use rouge_widgets::{NoticeTone, badge};

#[test]
fn a_badge_tints_its_tone_and_follows_the_theme_without_remounting() -> TestResult {
    let _entered = Runtime::new().enter();
    let (mut ui, theme) = component_test()
        .size(200.0, 60.0)
        .clear(Color::WHITE)
        .build_with(|| {
            let root = view! {
                <div p=10.0 flex>
                    <div test_id="badge">
                        <badge tone=NoticeTone::Positive>{"Synced".to_owned()}</badge>
                    </div>
                </div>
            };
            (root, Theme::current())
        });
    let at = ui.rect_of("badge")?;
    assert_eq!(at.size.height, 20.0);
    let (x, y) = (at.origin.x as u32 + 10, at.center().y as u32);
    let dot = |palette: UiPalette| palette.map(Color::from).positive.to_rgba8();
    assert_near(
        ui.pixel(x, y)?,
        dot(UiPalette::light()),
        "the dot is the tone",
    );
    let [red, green, blue, _] = ui.pixel(x + 6, y - 6)?;
    assert!(
        green > red && green > blue,
        "the fill is a tint of the tone"
    );
    assert!(ui.text_bounds("Synced").is_some());
    let created = ui.stats().elements_created;
    theme.set(UiPalette::dark());
    ui.run(0.5);
    assert_near(ui.pixel(x, y)?, dot(UiPalette::dark()), "dark tone");
    assert_eq!(ui.stats().elements_created, created, "no remount");
    assert_eq!(ui.compositor_frames(1.0)?, 0, "a badge never animates");
    Ok(())
}
