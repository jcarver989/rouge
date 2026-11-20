use crate::tokens::Space;
use crate::util::{TestId, chrome};
use crate::wheel::{Wheel, wheel};
use rouge_core::{
    Apply, BuildCx, DelegateStyle, Div, StyleDelta, Styled, View, component, div, view,
};
use rouge_reactive::{Signal, effect, signal};
use std::ops::RangeInclusive;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Date {
    pub const fn new(year: i32, month: u8, day: u8) -> Self {
        let month = if month < 1 {
            1
        } else if month > 12 {
            12
        } else {
            month
        };
        let length = Date::days_in_month(year, month);
        let day = if day < 1 {
            1
        } else if day > length {
            length
        } else {
            day
        };
        Date { year, month, day }
    }

    pub const fn days_in_month(year: i32, month: u8) -> u8 {
        match month {
            2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        }
    }
}

pub struct DatePicker {
    value: Signal<Date>,
    years: RangeInclusive<i32>,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn date_picker(value: Signal<Date>) -> DatePicker {
    let year = value.get_untracked().year;
    DatePicker {
        value,
        years: year - YEARS_AROUND..=year + YEARS_AROUND,
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl DatePicker {
    pub fn years(mut self, years: RangeInclusive<i32>) -> Self {
        self.years = years;
        self
    }
}

chrome!(DatePicker);

impl View for DatePicker {
    fn build(self, cx: &mut BuildCx<'_>) {
        let (value, id) = (self.value, self.test_id);
        let (first, last) = (
            *self.years.start(),
            (*self.years.end()).max(*self.years.start()),
        );
        let slot = move |date: Date| {
            (
                usize::from(date.month - 1),
                usize::from(date.day - 1),
                usize::try_from(date.year.clamp(first, last) - first).unwrap_or_default(),
            )
        };
        let (at_month, at_day, at_year) = slot(Date::new(
            value.get_untracked().year,
            value.get_untracked().month,
            value.get_untracked().day,
        ));
        let (month, day, year) = (signal(at_month), signal(at_day), signal(at_year));
        effect(move || {
            let held = value.get();
            let (at_month, at_day, at_year) = slot(Date::new(held.year, held.month, held.day));
            month.set_if_changed(at_month);
            day.set_if_changed(at_day);
            year.set_if_changed(at_year);
        });
        effect(move || {
            let picked = Date::new(
                first + i32::try_from(year.get()).unwrap_or_default(),
                u8::try_from(month.get() + 1).unwrap_or(12),
                u8::try_from(day.get() + 1).unwrap_or(31),
            );
            day.set_if_changed(usize::from(picked.day - 1));
            value.set_if_changed(picked);
        });
        let days = (1..=31).map(|day| day.to_string());
        let years = (first..=last).map(|year| year.to_string());
        let wheels = view! {
            <div flex gap=Space::X1>
                {wheel(month, MONTHS).when_some(id.part("month"), Wheel::test_id).flex_1()}
                {wheel(day, days).when_some(id.part("day"), Wheel::test_id).w(DAY_WIDTH)}
                {wheel(year, years).when_some(id.part("year"), Wheel::test_id).w(YEAR_WIDTH)}
            </div>
        };
        let root = wheels.when_some(id, Div::test_id);
        self.style.apply(root).build(cx);
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const YEARS_AROUND: i32 = 50;
const DAY_WIDTH: f32 = 64.0;
const YEAR_WIDTH: f32 = 88.0;
