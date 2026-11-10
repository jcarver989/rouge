mod common;

use common::builder;
use rouge_core::testing::TestResult;
use rouge_core::{Key, Styled, div};
use rouge_reactive::{Runtime, on_cleanup, signal};
use rouge_widgets::tabs;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn tabs_mount_only_the_selected_panel_and_switch_by_click_and_arrows() -> TestResult {
    let _entered = Runtime::new().enter();
    let selected = signal(0usize);
    let log = Rc::new(RefCell::new(Vec::new()));
    let (first, second) = (log.clone(), log.clone());
    let mut ui = builder().build(move || {
        tabs(selected)
            .tab("General", move || {
                first.borrow_mut().push("mount general");
                let log = first.clone();
                on_cleanup(move || log.borrow_mut().push("dispose general"));
                div().w(120.0).h(40.0)
            })
            .tab("Advanced", move || {
                second.borrow_mut().push("mount advanced");
                div().w(60.0).h(40.0)
            })
            .size_full()
    });
    assert_eq!(*log.borrow(), ["mount general"]);
    let strip = ui.bounds(10.0, 10.0)?.size.height;
    assert_eq!(ui.bounds(5.0, strip + 20.0)?.size.width, 120.0);
    let general = ui.bounds(10.0, 10.0)?.size.width;
    ui.click_at(general + 30.0, 10.0);
    assert_eq!(selected.get(), 1);
    assert_eq!(
        *log.borrow(),
        ["mount general", "dispose general", "mount advanced"]
    );
    assert_eq!(ui.bounds(5.0, strip + 20.0)?.size.width, 60.0);
    ui.press(Key::ArrowRight);
    assert_eq!(selected.get(), 0, "arrows wrap around");
    ui.press(Key::ArrowLeft);
    assert_eq!(selected.get(), 1);
    Ok(())
}
