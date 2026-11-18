use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{ImageData, Styled, div};
use rouge_reactive::Runtime;
use rouge_widgets::testing::component_test;
use rouge_widgets::{avatar, avatar_group};

#[test]
fn an_avatar_shows_initials_or_an_image() -> TestResult {
    let _entered = Runtime::new().enter();
    let red = ImageData::rgba8(1, 1, vec![255, 0, 0, 255])?;
    let mut ui = component_test().size(200.0, 80.0).build(|| {
        view! {
            <div p=10.0 flex gap=8.0>
                <avatar name="ines okafor".to_owned() test_id="ines"/>
                <avatar name="Marek Havel".to_owned() test_id="marek" image=red avatar_size=40.0/>
            </div>
        }
    });
    ui.run(0.2);
    assert_eq!(ui.rect_of("ines")?.size.width, 28.0);
    assert!(ui.text_bounds("IO").is_some(), "two upper-case initials");
    let marek = ui.rect_of("marek")?;
    assert_eq!(marek.size.height, 40.0);
    assert!(
        ui.text_bounds("MH").is_none(),
        "the image replaces the initials"
    );
    let [red, green, _, _] = ui.pixel(marek.center().x as u32, marek.center().y as u32)?;
    assert!(red > 200 && green < 50, "the image fills the disc");
    assert_eq!(
        ui.pixel(marek.origin.x as u32, marek.origin.y as u32)?,
        ui.pixel(1, 1)?,
        "and is clipped to it"
    );
    Ok(())
}

#[test]
fn a_group_overlaps_its_avatars_and_counts_the_overflow() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test().size(300.0, 80.0).build(|| {
        let people = [
            "Ines Okafor",
            "Marek Havel",
            "Yuki Tan",
            "Sal Choir",
            "Kofi Lund",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            view! {
                <avatar name=name.to_owned() test_id={format!("person-{index}")}/>
            }
        });
        view! {
            <div p=10.0 flex>
                <avatar_group avatars=people max=3 test_id="people"/>
            </div>
        }
    });
    let (first, second) = (ui.rect_of("person-0")?, ui.rect_of("person-1")?);
    assert_eq!(
        second.origin.x,
        first.max().x - 4.0,
        "each overlaps the last"
    );
    assert!(
        ui.bounds_of("person-3").is_none(),
        "only max avatars are drawn"
    );
    assert!(ui.rect_of("people-overflow")?.origin.x > ui.rect_of("person-2")?.origin.x);
    assert!(ui.text_bounds("+2").is_some());
    Ok(())
}
