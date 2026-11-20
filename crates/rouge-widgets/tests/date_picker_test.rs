use rouge_core::testing::TestResult;
use rouge_core::view;
use rouge_core::{InputKind, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::{Date, date_picker};

#[test]
fn a_date_is_always_a_real_day() {
    assert_eq!(Date::new(2026, 2, 31), Date::new(2026, 2, 28));
    assert_eq!(Date::new(2024, 2, 31).day, 29, "leap years have a 29th");
    assert_eq!(
        Date::new(1900, 2, 29).day,
        28,
        "centuries are not leap years"
    );
    assert_eq!(Date::new(2000, 2, 29).day, 29, "unless divisible by 400");
    assert_eq!(Date::new(2026, 0, 0), Date::new(2026, 1, 1));
    assert_eq!(Date::new(2026, 13, 40), Date::new(2026, 12, 31));
    assert!(Date::new(2026, 11, 20) < Date::new(2026, 12, 1));
}

#[test]
fn three_wheels_show_the_date_follow_it_and_never_leave_an_impossible_day() -> TestResult {
    let _entered = Runtime::new().enter();
    let due = signal(Date::new(2026, 1, 31));
    let mut ui = component_test()
        .size(390.0, 260.0)
        .input(InputKind::Touch)
        .build(move || {
            view! {
                <div flex col p=16.0 size_full>
                    <date_picker value=due test_id="due" years={2020..=2030}/>
                </div>
            }
        });
    ui.run(0.5);
    let band = ui.rect_of("due-month-band")?;
    assert_eq!(ui.rect_of("due-month-row-0")?.origin.y, band.origin.y);
    assert_eq!(ui.rect_of("due-day-row-30")?.origin.y, band.origin.y);
    assert_eq!(ui.rect_of("due-year-row-6")?.origin.y, band.origin.y);
    assert!(ui.text_bounds("January").is_some() && ui.text_bounds("2026").is_some());
    let (month, day, year) = (
        ui.rect_of("due-month")?,
        ui.rect_of("due-day")?,
        ui.rect_of("due-year")?,
    );
    assert!(month.max().x <= day.origin.x && day.max().x <= year.origin.x);
    assert_eq!(year.max().x, 374.0, "the wheels fill the row");
    ui.tap_on("due-month-row-1")?;
    ui.run(1.0);
    assert_eq!(
        due.get(),
        Date::new(2026, 2, 28),
        "February has no 31st, so the day comes back"
    );
    assert_eq!(ui.rect_of("due-day-row-27")?.origin.y, band.origin.y);
    due.set(Date::new(2028, 2, 29));
    ui.run(1.0);
    assert_eq!(ui.rect_of("due-year-row-8")?.origin.y, band.origin.y);
    assert_eq!(ui.rect_of("due-day-row-28")?.origin.y, band.origin.y);
    Ok(())
}
