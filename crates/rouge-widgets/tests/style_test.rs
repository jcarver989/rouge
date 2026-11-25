use rouge_core::testing::{TestResult, TestUi, assert_near};
use rouge_core::{Color, Styled, div, text, view};
use rouge_reactive::Runtime;
use rouge_theme::UiPalette;
use rouge_widgets::testing::component_test;
use rouge_widgets::{ButtonVariant, CODE_FONT, Fonts, Radii, UI_FONT, UiStyle, UiStyles, button};

#[test]
fn branded_widgets_use_host_corners_and_font_roles() -> TestResult {
    let _entered = Runtime::new().enter();
    let mount = |style: Option<UiStyle>| {
        component_test().size(240.0, 160.0).build(move || {
            if let Some(style) = style {
                style.provide();
            }
            view! {
                <div flex col items_start bg=Color::WHITE>
                    <button test_id="action" label="Save" variant=ButtonVariant::Primary/>
                    <div test_id="ui"><text body>"iiiiiiiiiiii"</text></div>
                    <div test_id="code"><text code>"iiiiiiiiiiii"</text></div>
                </div>
            }
        })
    };
    let mut default = mount(None);
    let mut brand = mount(Some(UiStyle {
        fonts: swapped_fonts(),
        radii: Radii::SHARP,
        ..UiStyle::default()
    }));
    let width = |ui: &mut TestUi, id| ui.rect_of(id).map(|r| r.size.width);
    assert!(width(&mut brand, "ui")? > width(&mut default, "ui")? * 1.5);
    assert!(width(&mut brand, "code")? * 1.5 < width(&mut default, "code")?);
    let corner = brand.rect_of("action")?.origin;
    assert_near(
        brand.pixel(corner.x as u32 + 1, corner.y as u32 + 1)?,
        Color::from(UiPalette::default().accent).to_rgba8(),
        "square button corner",
    );
    Ok(())
}

#[cfg(feature = "markdown")]
#[test]
fn markdown_headings_use_the_host_ui_font() -> TestResult {
    use rouge_markdown::MarkdownDocument;
    use rouge_widgets::markdown;
    let _entered = Runtime::new().enter();
    let width = |fonts| {
        let ui = component_test().size(400.0, 200.0).build(|| {
            UiStyle {
                fonts,
                ..UiStyle::default()
            }
            .provide();
            markdown(MarkdownDocument::parse("# iiiiiiiiiiiii"))
        });
        ui.text_bounds("iiiiiiiiiiiii")
            .map(|r| r.size.width)
            .ok_or("missing heading")
    };
    assert!(width(swapped_fonts())? > width(Fonts::default())? * 1.3);
    Ok(())
}

fn swapped_fonts() -> Fonts {
    Fonts {
        ui: CODE_FONT.into(),
        code: UI_FONT.into(),
    }
}
