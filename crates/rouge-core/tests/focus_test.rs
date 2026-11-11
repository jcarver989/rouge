use rouge_core::testing::{
    BLUE, GREEN, RED, TestResult, TestUi, TestUiBuilder, assert_near, counter,
};
use rouge_core::{
    Chord, ChordError, Color, Cursor, FocusHandle, ImeEvent, Interactions, Key, KeyEvent,
    Modifiers, Point, Styled, div, focus_handle, show,
};
use rouge_reactive::{Runtime, signal};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn tab_moves_focus_in_order_and_wraps() {
    let _rt = Runtime::new().enter();
    let (mut ui, handles) = TestUiBuilder::default().build_with(|| {
        let stops = [focus_handle(), focus_handle(), focus_handle()];
        let root = div()
            .child(div().track_focus(stops[0]))
            .child(div().track_focus(stops[1]).tab_index(1))
            .child(div().focusable().enabled(false))
            .child(div().track_focus(stops[2]))
            .child(div().focusable().tab_index(-1));
        (root, stops)
    });
    assert!(!ui.has_focus());
    assert!(ui.key_down(KeyEvent::new(Key::Tab)));
    assert_eq!(
        focused(&handles),
        [false, true, false],
        "positive tab index goes first"
    );
    ui.key_down(KeyEvent::new(Key::Tab));
    assert_eq!(focused(&handles), [true, false, false]);
    ui.key_down(KeyEvent::new(Key::Tab));
    assert_eq!(
        focused(&handles),
        [false, false, true],
        "disabled and negative stops are skipped"
    );
    ui.key_down(KeyEvent::new(Key::Tab));
    assert_eq!(focused(&handles), [false, true, false], "wraps around");
    ui.key_down(KeyEvent::new(Key::Tab).with_modifiers(Modifiers::SHIFT));
    assert_eq!(focused(&handles), [false, false, true], "shift reverses");
}

#[test]
fn focus_variants_distinguish_keyboard_from_pointer() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div().size_full().child(
            div()
                .size(50.0)
                .bg(Color::RED)
                .focusable()
                .focus(|s| s.bg(Color::BLUE))
                .focus_visible(|s| s.bg(Color::GREEN)),
        )
    });
    assert_near(ui.pixel(10, 10)?, RED, "unfocused");
    ui.click(Point::new(10.0, 10.0));
    ui.advance(0.0);
    assert_near(
        ui.pixel(10, 10)?,
        BLUE,
        "pointer focus is not focus-visible",
    );
    ui.click(Point::new(150.0, 80.0));
    ui.advance(0.0);
    assert_near(ui.pixel(10, 10)?, RED, "clicking elsewhere blurs");
    ui.key_down(KeyEvent::new(Key::Tab));
    ui.advance(0.0);
    assert_near(ui.pixel(10, 10)?, GREEN, "keyboard focus is focus-visible");
    Ok(())
}

#[test]
fn key_events_bubble_from_the_focused_element_and_can_be_stopped() {
    let _rt = Runtime::new().enter();
    let log = Rc::new(RefCell::new(Vec::new()));
    let (inner, outer, ups) = (log.clone(), log.clone(), log.clone());
    let mut ui = TestUi::new(move || {
        div()
            .size_full()
            .on_key_down(move |event| outer.borrow_mut().push(format!("outer {:?}", event.key)))
            .on_key_up(move |event| ups.borrow_mut().push(format!("up {:?}", event.key)))
            .child(div().size(50.0).focusable().on_key_down(move |event| {
                inner.borrow_mut().push(format!("inner {:?}", event.key));
                if event.key == Key::Enter {
                    event.stop_propagation();
                }
            }))
    });
    assert!(
        !ui.key_down(KeyEvent::typed("a")),
        "unhandled without focus"
    );
    assert_eq!(
        *log.borrow(),
        ["outer Character(\"a\")"],
        "the root view sees keys when nothing is focused"
    );
    log.borrow_mut().clear();
    ui.click(Point::new(10.0, 10.0));
    assert!(!ui.key_down(KeyEvent::new(Key::Escape)));
    assert!(
        ui.key_down(KeyEvent::new(Key::Enter)),
        "stopped events count as handled"
    );
    ui.key_up(KeyEvent::new(Key::Enter));
    assert_eq!(
        *log.borrow(),
        ["inner Escape", "outer Escape", "inner Enter", "up Enter"]
    );
}

#[test]
fn key_bindings_dispatch_typed_actions_along_the_focus_path() -> Result<(), ChordError> {
    let _rt = Runtime::new().enter();
    let (cmd_s, escape): (Chord, Chord) = ("cmd-s".parse()?, "escape".parse()?);
    let saved = Rc::new(RefCell::new(Vec::new()));
    let (closed, close) = counter();
    let record = saved.clone();
    let mut ui = TestUi::new(move || {
        div()
            .size_full()
            .bind_key(cmd_s.clone(), Save(1))
            .bind_key(escape, Close)
            .on_action(move |save: &Save| record.borrow_mut().push(save.clone()))
            .on_action(move |_: &Close| close())
            .child(div().size(50.0).focusable().bind_key(cmd_s, Save(2)))
            .child(
                div()
                    .size(50.0)
                    .focusable()
                    .on_key_down(KeyEvent::stop_propagation),
            )
    });
    let save = || KeyEvent::new(Key::character("s")).with_modifiers(Modifiers::COMMAND);
    assert!(ui.key_down(save()));
    assert!(!ui.key_down(KeyEvent::typed("s")), "modifiers must match");
    ui.click(Point::new(10.0, 10.0));
    ui.key_down(save());
    ui.key_down(KeyEvent::new(Key::Escape));
    assert_eq!(
        *saved.borrow(),
        [Save(1), Save(2)],
        "the deepest binding on the focus path wins"
    );
    assert_eq!(closed.get(), 1);
    ui.click(Point::new(10.0, 60.0));
    ui.key_down(save());
    assert_eq!(
        saved.borrow().len(),
        2,
        "a key handler that stops the event suppresses bindings"
    );
    Ok(())
}

#[test]
fn focus_handles_focus_blur_and_report_events() {
    let _rt = Runtime::new().enter();
    let log = Rc::new(RefCell::new(Vec::new()));
    let (focus, blur) = (log.clone(), log.clone());
    let (mut ui, handle) = TestUiBuilder::default().build_with(move || {
        let handle = focus_handle();
        let root = div()
            .track_focus(handle)
            .on_focus(move |event| focus.borrow_mut().push(("focus", event.visible)))
            .on_blur(move |_| blur.borrow_mut().push(("blur", false)));
        (root, handle)
    });
    handle.focus();
    assert!(ui.has_focus());
    assert!(handle.is_focused());
    handle.blur();
    assert!(!ui.has_focus());
    assert_eq!(*log.borrow(), [("focus", false), ("blur", false)]);
}

#[test]
fn autofocus_takes_focus_inside_a_scope_and_restores_it_when_removed() {
    let _rt = Runtime::new().enter();
    let open = signal(false);
    let (mut ui, handles) = TestUiBuilder::default().build_with(move || {
        let (trigger, first, second) = (focus_handle(), focus_handle(), focus_handle());
        let scope = move || {
            div()
                .focus_scope()
                .child(div().track_focus(first).autofocus())
                .child(div().track_focus(second))
        };
        let root = div()
            .child(div().track_focus(trigger))
            .child(show(move || open.get(), scope));
        (root, [trigger, first, second])
    });
    ui.key_down(KeyEvent::new(Key::Tab));
    assert_eq!(focused(&handles), [true, false, false]);
    open.set(true);
    ui.advance(0.0);
    assert_eq!(
        focused(&handles),
        [false, true, false],
        "autofocus moves focus in"
    );
    ui.key_down(KeyEvent::new(Key::Tab));
    ui.key_down(KeyEvent::new(Key::Tab));
    assert_eq!(
        focused(&handles),
        [false, true, false],
        "tab is trapped inside the scope"
    );
    open.set(false);
    ui.advance(0.0);
    assert!(focused(&handles)[0], "focus returns to where it was");
}

#[test]
fn removing_a_scope_does_not_restore_focus_to_an_element_removed_in_the_same_update() {
    let _rt = Runtime::new().enter();
    let (alive, open) = (signal(true), signal(false));
    let (mut ui, editor) = TestUiBuilder::default().build_with(move || {
        let editor = focus_handle();
        let scope = move || {
            div()
                .focus_scope()
                .child(div().track_focus(editor).autofocus())
        };
        let trigger = move || {
            let state = signal(Interactions::default());
            div().focusable().track_interaction(state)
        };
        let root = div()
            .child(show(move || alive.get(), trigger))
            .child(show(move || alive.get() && open.get(), scope));
        (root, editor)
    });
    ui.key_down(KeyEvent::new(Key::Tab));
    open.set(true);
    ui.advance(0.0);
    assert!(editor.is_focused());
    alive.set(false);
    ui.advance(0.0);
    assert!(!editor.is_focused());
}

#[test]
fn disabled_elements_ignore_pointer_focus_and_drop_interactive_styles() -> TestResult {
    let _rt = Runtime::new().enter();
    let enabled = signal(false);
    let (clicks, click) = counter();
    let (outer_clicks, outer_click) = counter();
    let mut ui = TestUi::new(move || {
        div().size_full().on_click(move |_| outer_click()).child(
            div()
                .size(50.0)
                .bg(Color::RED)
                .focusable()
                .enabled(enabled)
                .hover(|s| s.bg(Color::BLUE))
                .disabled(|s| s.bg(Color::GREEN))
                .on_click(move |_| click()),
        )
    });
    ui.click(Point::new(10.0, 10.0));
    ui.advance(0.0);
    assert_eq!(
        (clicks.get(), outer_clicks.get()),
        (0, 1),
        "ancestors still receive the click"
    );
    assert!(!ui.has_focus());
    assert_near(ui.pixel(10, 10)?, GREEN, "disabled style wins over hover");
    enabled.set(true);
    ui.advance(0.0);
    assert_near(ui.pixel(10, 10)?, BLUE, "hover applies again once enabled");
    ui.click(Point::new(10.0, 10.0));
    assert_eq!(clicks.get(), 1);
    assert!(ui.has_focus());
    enabled.set(false);
    ui.advance(0.0);
    assert!(!ui.has_focus(), "disabling the focused element blurs it");
    Ok(())
}

#[test]
fn ime_events_reach_the_focused_element_and_cursor_follows_hover() {
    let _rt = Runtime::new().enter();
    let committed = Rc::new(RefCell::new(String::new()));
    let sink = committed.clone();
    let mut ui = TestUi::new(move || {
        div()
            .size_full()
            .child(
                div()
                    .size(50.0)
                    .focusable()
                    .cursor(Cursor::Text)
                    .on_ime(move |event| {
                        if let ImeEvent::Commit(text) = event {
                            sink.borrow_mut().push_str(text);
                        }
                    }),
            )
    });
    assert!(!ui.accepts_text());
    assert_eq!(ui.cursor(), Cursor::Default);
    ui.pointer_moved(Point::new(10.0, 10.0));
    assert_eq!(ui.cursor(), Cursor::Text);
    ui.click(Point::new(10.0, 10.0));
    assert!(ui.accepts_text());
    ui.ime(ImeEvent::Commit("日本".into()));
    assert_eq!(*committed.borrow(), "日本");
}

fn focused(handles: &[FocusHandle]) -> Vec<bool> {
    handles.iter().map(FocusHandle::is_focused).collect()
}

#[derive(Clone, Debug, PartialEq)]
struct Save(u32);

struct Close;
