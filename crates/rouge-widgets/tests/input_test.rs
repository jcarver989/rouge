mod common;

use common::builder;
use rouge_core::testing::{TestError, TestResult, TestUi, assert_near};
use rouge_core::{Key, Palette, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{text_area, text_input};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn text_input_edits_its_signal_shows_focus_and_submits() -> TestResult {
    let _entered = Runtime::new().enter();
    let value = signal(String::new());
    let submitted = Rc::new(RefCell::new(Vec::new()));
    let sink = submitted.clone();
    let mut ui = builder().build(move || {
        let field = text_input(value)
            .placeholder("Name")
            .w(200.0)
            .on_submit(move |text| sink.borrow_mut().push(text.to_owned()));
        div().p(10.0).child(field)
    });
    let palette = Palette::light();
    assert_near(
        ui.render()?.pixel(100, 10),
        palette.border.to_rgba8(),
        "idle border",
    );
    ui.click_at(12.0, 12.0);
    assert!(ui.accepts_text(), "clicking the padding focuses the field");
    assert_near(
        ui.render()?.pixel(100, 10),
        palette.accent.to_rgba8(),
        "focused border",
    );
    ui.type_text("Ada");
    ui.press(Key::Enter);
    assert_eq!(value.get(), "Ada");
    assert_eq!(*submitted.borrow(), ["Ada"]);
    Ok(())
}

#[test]
fn placeholder_is_shown_only_while_empty_and_text_area_takes_newlines() -> TestResult {
    let _entered = Runtime::new().enter();
    let (name, notes) = (signal(String::new()), signal(String::new()));
    let mut ui = builder().build(move || {
        div()
            .p(10.0)
            .flex()
            .col()
            .gap(10.0)
            .child(text_input(name).placeholder("MMMMMM").w(200.0))
            .child(text_area(notes).w(200.0))
    });
    let dark = |ui: &mut TestUi| -> Result<usize, TestError> {
        let img = ui.render()?;
        Ok((20..120).filter(|x| img.pixel(*x, 26)[0] < 200).count())
    };
    assert!(dark(&mut ui)? > 10, "the placeholder is drawn");
    name.set("i".into());
    ui.advance(0.0);
    assert!(
        dark(&mut ui)? < 6,
        "the placeholder is gone once there is text"
    );
    ui.click_at(100.0, 90.0);
    ui.type_text("a");
    ui.press(Key::Enter);
    ui.type_text("b");
    assert_eq!(notes.get(), "a\nb");
    Ok(())
}
