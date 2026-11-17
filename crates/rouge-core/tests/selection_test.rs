use rouge_core::testing::{TestResult, TestUi, TestUiBuilder};
use rouge_core::{
    Align, Cursor, EditAction, MemoryClipboard, Modifiers, Point, PointerButton, Rect, RowKey,
    Styled, TextBreak, View, VirtualListHandle, div, text, virtual_list, virtual_list_handle,
};
use rouge_reactive::{Runtime, Signal, signal};

#[test]
fn dragging_across_texts_selects_and_copies_them_joined_by_lines() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(two_texts);
    scene
        .ui
        .drag(scene.start_of("beta")?, scene.end_of("gamma")?);
    assert_eq!(scene.copy().as_deref(), Some("beta\ngamma"));
    Ok(())
}

#[test]
fn a_selection_is_painted_behind_the_selected_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(two_texts);
    let before = scene.ui.render()?;
    let area = scene.bounds("beta")?;
    scene
        .ui
        .drag(scene.start_of("beta")?, scene.end_of("beta")?);
    let after = scene.ui.render()?;
    let changed = (area.origin.y as u32..area.max().y as u32)
        .flat_map(|y| (area.origin.x as u32..area.max().x as u32).map(move |x| (x, y)))
        .filter(|(x, y)| before.pixel(*x, *y) != after.pixel(*x, *y))
        .count();
    assert!(changed > 0, "the selected word is highlighted");
    Ok(())
}

#[test]
fn double_click_selects_a_word_and_triple_click_the_whole_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(two_texts);
    let word = scene.bounds("beta")?.center();
    scene.ui.click_times(word, 2);
    assert_eq!(scene.copy().as_deref(), Some("beta"));
    scene.ui.click_times(word, 3);
    assert_eq!(scene.copy().as_deref(), Some("alpha beta"));
    Ok(())
}

#[test]
fn dragging_after_a_double_click_extends_by_whole_words() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(two_texts);
    let word = scene.bounds("beta")?.center();
    let delta = scene.bounds("delta")?;
    scene.ui.click_times(word, 1);
    scene.ui.pointer_down(PointerButton::Primary);
    scene
        .ui
        .pointer_moved(Point::new(delta.origin.x + 2.0, delta.center().y));
    scene.ui.pointer_up(PointerButton::Primary);
    scene.ui.advance(0.0);
    assert_eq!(scene.copy().as_deref(), Some("beta\ngamma delta"));
    Ok(())
}

#[test]
fn shift_click_extends_the_selection_from_its_anchor() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(two_texts);
    scene.ui.click_times(scene.start_of("alpha")?, 1);
    scene.ui.set_modifiers(Modifiers::SHIFT);
    scene.ui.click_times(scene.end_of("delta")?, 1);
    scene.ui.set_modifiers(Modifiers::NONE);
    assert_eq!(scene.copy().as_deref(), Some("alpha beta\ngamma delta"));
    Ok(())
}

#[test]
fn a_click_or_escape_clears_the_selection() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(two_texts);
    scene
        .ui
        .drag(scene.start_of("alpha")?, scene.end_of("beta")?);
    assert!(scene.copy().is_some());
    scene.ui.press("escape");
    assert_eq!(scene.copy(), None);

    scene
        .ui
        .drag(scene.start_of("alpha")?, scene.end_of("beta")?);
    scene.ui.click_times(scene.bounds("gamma")?.center(), 1);
    assert_eq!(scene.copy(), None, "nothing is copied without a selection");
    Ok(())
}

#[test]
fn select_all_copies_every_text_with_its_separator_and_skips_unselectable_ones() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(|| {
        div()
            .flex()
            .col()
            .child(
                div()
                    .flex()
                    .child(text("name"))
                    .child(text("age").selection_break(TextBreak::Tab)),
            )
            .child(text("hidden chrome").unselectable())
            .child(
                div()
                    .flex()
                    .child(text("•"))
                    .child(text("item").selection_break(TextBreak::Space)),
            )
            .child(div().unselectable().child(text("more chrome")))
            .child(text("closing").selection_break(TextBreak::Paragraph))
    });
    scene.ui.click_times(scene.start_of("name")?, 1);
    scene.ui.press("cmd-a");
    assert_eq!(
        scene.copy().as_deref(),
        Some("name\tage\n• item\n\nclosing")
    );
    Ok(())
}

#[test]
fn the_edit_menu_copy_action_copies_the_selection() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(two_texts);
    scene
        .ui
        .drag(scene.start_of("gamma")?, scene.end_of("delta")?);
    assert!(scene.ui.dispatch_action(EditAction::Copy));
    assert_eq!(scene.clipboard.text().as_deref(), Some("gamma delta"));
    Ok(())
}

#[test]
fn clicks_inside_the_scope_still_reach_their_handlers() -> TestResult {
    let _rt = Runtime::new().enter();
    let clicked = signal(0);
    let mut scene = Scene::new(move || {
        div().flex().col().child(text("alpha beta")).child(
            div()
                .test_id("button")
                .h(20.0)
                .on_click(move |_| clicked.update(|count| *count += 1))
                .child(text("press")),
        )
    });
    scene
        .ui
        .drag(scene.start_of("alpha")?, scene.end_of("beta")?);
    scene.ui.click_on("button")?;
    assert_eq!(clicked.get(), 1);
    assert_eq!(scene.copy(), None);
    Ok(())
}

#[test]
fn text_inside_an_activatable_control_is_left_out_of_a_selection() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(|| {
        div()
            .flex()
            .col()
            .child(text("before"))
            .child(div().on_activate(|| {}).child(text("toggle")))
            .child(text("after"))
    });
    scene.ui.click_times(scene.start_of("before")?, 1);
    scene.ui.press("cmd-a");
    assert_eq!(scene.copy().as_deref(), Some("before\nafter"));
    Ok(())
}

#[test]
fn the_pointer_is_a_text_cursor_only_over_selectable_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(|| {
        div()
            .flex()
            .col()
            .child(text("words").test_id("words"))
            .child(text("chrome").test_id("chrome").unselectable())
            .child(text("link").test_id("link").cursor(Some(Cursor::Pointer)))
    });
    for (name, cursor) in [
        ("words", Cursor::Text),
        ("chrome", Cursor::default()),
        ("link", Cursor::Pointer),
    ] {
        scene.ui.hover_on(name)?;
        assert_eq!(scene.ui.cursor(), cursor, "over {name}");
    }
    Ok(())
}

#[test]
fn copying_rows_that_scrolled_out_of_a_virtual_list_uses_snapshots_and_row_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let list = virtual_list_handle();
    let mut scene = Scene::new(move || rows(list, signal(0)));
    let number = scene.ui.rect_of("number-1")?;
    scene
        .ui
        .click_times(Point::new(number.origin.x + 0.5, number.center().y), 1);
    list.scroll_to_row(50, Align::Top);
    scene.ui.advance(0.0);
    assert!(
        scene.ui.bounds_of("row-1").is_none(),
        "the first row is unmounted"
    );
    let final_number = scene.ui.rect_of("number-52")?;
    scene.ui.set_modifiers(Modifiers::SHIFT);
    scene.ui.click_times(
        Point::new(final_number.max().x - 0.5, final_number.center().y),
        1,
    );
    scene.ui.set_modifiers(Modifiers::NONE);

    let middle: Vec<String> = (2..52).map(|row| format!("row {row}")).collect();
    let expected = format!("1\n\n{}\n\nrow 52", middle.join("\n\n"));
    assert_eq!(scene.copy().as_deref(), Some(expected.as_str()));
    Ok(())
}

#[test]
fn dragging_past_the_bottom_of_a_list_scrolls_it_and_extends_the_selection() -> TestResult {
    let _rt = Runtime::new().enter();
    let list = virtual_list_handle();
    let mut scene = Scene::new(move || rows(list, signal(0)));
    let from = scene.start_of("row")?;
    scene.ui.advance(1.0);
    scene.ui.pointer_moved(from);
    scene.ui.pointer_down(PointerButton::Primary);
    scene
        .ui
        .pointer_moved(Point::new(from.x, LIST_HEIGHT + 40.0));
    for _ in 0..60 {
        if list.scroll_state().get_untracked().offset.y >= 200.0 {
            break;
        }
        scene.ui.advance(0.05);
    }
    assert!(list.scroll_state().get_untracked().offset.y >= 200.0);
    scene.ui.pointer_up(PointerButton::Primary);
    scene.ui.advance(0.0);
    let copied = scene.copy().ok_or("nothing selected")?;
    assert!(copied.starts_with("row 0\n\nrow 1"), "{copied}");
    assert!(copied.contains("row 12"), "{copied}");
    Ok(())
}

#[test]
fn scrolling_a_list_mid_drag_extends_the_selection_to_the_row_under_the_pointer() -> TestResult {
    let _rt = Runtime::new().enter();
    let list = virtual_list_handle();
    let mut scene = Scene::new(move || rows(list, signal(0)));
    let from = scene.start_of("row")?;
    scene.ui.advance(1.0);
    scene.ui.pointer_moved(from);
    scene.ui.pointer_down(PointerButton::Primary);
    scene.ui.pointer_moved(Point::new(from.x, ROW_HEIGHT / 2.0));
    list.scroll_to_row(20, Align::Top);
    scene.ui.advance(0.0);
    scene.ui.pointer_up(PointerButton::Primary);
    scene.ui.advance(0.0);
    let copied = scene.copy().ok_or("nothing selected")?;
    assert!(copied.starts_with("row 0\n\nrow 1"), "{copied}");
    assert!(copied.ends_with("row 19"), "{copied}");
    Ok(())
}

#[test]
fn a_row_replaced_under_a_selection_drops_it_even_when_scrolled_out_of_view() -> TestResult {
    let _rt = Runtime::new().enter();
    let list = virtual_list_handle();
    let revision = signal(0);
    let mut scene = Scene::new(move || rows(list, revision));
    scene.ui.drag(scene.start_of("row")?, scene.end_of("1")?);
    list.scroll_to_row(50, Align::Top);
    scene.ui.advance(0.0);
    assert!(scene.copy().is_some());
    revision.set(1);
    scene.ui.advance(0.0);
    assert_eq!(scene.copy(), None);
    Ok(())
}

#[test]
fn a_selection_uses_the_list_it_started_in_and_does_not_hide_plain_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut scene = Scene::new(|| {
        let list = |label: &'static str| {
            div().h(ROW_HEIGHT).child(
                virtual_list(|| 3, ROW_HEIGHT, move |row| text(format!("{label} {row}")))
                    .row_text(move |row| format!("{label} {row}"))
                    .size_full(),
            )
        };
        div()
            .flex()
            .col()
            .child(list("first"))
            .child(list("second"))
            .child(text("outside"))
    });
    scene.ui.click_text("second 0")?;
    scene.ui.press("cmd-a");
    assert_eq!(
        scene.copy().as_deref(),
        Some("second 0\n\nsecond 1\n\nsecond 2")
    );
    scene.ui.click_text("outside")?;
    scene.ui.press("cmd-a");
    assert_eq!(scene.copy().as_deref(), Some("outside"));
    Ok(())
}

#[test]
fn shrinking_an_unkeyed_list_invalidates_a_selection_past_its_new_end() -> TestResult {
    let _rt = Runtime::new().enter();
    let size = signal(25);
    let mut scene = Scene::new(move || {
        div().h(LIST_HEIGHT).child(
            virtual_list(
                move || size.get(),
                ROW_HEIGHT,
                |row| text(format!("entry {row}")),
            )
            .row_text(move |row| {
                assert!(row < size.get_untracked());
                format!("entry {row}")
            })
            .size_full(),
        )
    });
    scene.ui.click_text("entry 0")?;
    scene.ui.press("cmd-a");
    assert!(scene.copy().is_some_and(|text| text.ends_with("entry 24")));
    size.set(1);
    scene.ui.advance(0.0);
    assert_eq!(scene.copy(), None);
    Ok(())
}

const ROWS: usize = 100;
const ROW_HEIGHT: f32 = 20.0;
const LIST_HEIGHT: f32 = 100.0;

fn two_texts() -> impl View {
    div()
        .flex()
        .col()
        .gap(8.0)
        .child(text("alpha beta"))
        .child(text("gamma delta"))
}

fn rows(list: VirtualListHandle, revision: Signal<u64>) -> impl View {
    div().h(LIST_HEIGHT).w_full().child(
        virtual_list(
            || ROWS,
            ROW_HEIGHT,
            |row| {
                div()
                    .test_id(format!("row-{row}"))
                    .h(ROW_HEIGHT)
                    .flex()
                    .gap(4.0)
                    .child(text("row"))
                    .child(
                        text(format!("{row}"))
                            .test_id(format!("number-{row}"))
                            .selection_break(TextBreak::Space),
                    )
            },
        )
        .row_identity(move |row| RowKey::new(row, revision.get()))
        .row_text(|row| format!("row {row}"))
        .handle(list)
        .size_full(),
    )
}

struct Scene {
    ui: TestUi,
    clipboard: MemoryClipboard,
}

impl Scene {
    fn new<T: View>(content: impl FnOnce() -> T + 'static) -> Self {
        let mut ui = TestUiBuilder::default()
            .size(400.0, 200.0)
            .build(move || div().size_full().selectable().child(content()));
        let clipboard = MemoryClipboard::new();
        ui.set_clipboard(clipboard.clone());
        Self { ui, clipboard }
    }

    fn copy(&mut self) -> Option<String> {
        self.clipboard.set_text("");
        self.ui.press("cmd-c");
        self.clipboard.text().filter(|text| !text.is_empty())
    }

    fn start_of(&self, needle: &str) -> Result<Point, String> {
        let rect = self.bounds(needle)?;
        Ok(Point::new(rect.origin.x + 0.5, rect.center().y))
    }

    fn end_of(&self, needle: &str) -> Result<Point, String> {
        let rect = self.bounds(needle)?;
        Ok(Point::new(rect.max().x - 0.5, rect.center().y))
    }

    fn bounds(&self, needle: &str) -> Result<Rect, String> {
        self.ui
            .text_bounds(needle)
            .ok_or_else(|| format!("no text {needle:?}"))
    }
}
