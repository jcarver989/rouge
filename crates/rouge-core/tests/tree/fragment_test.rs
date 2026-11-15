use rouge_core::Wake;
use rouge_core::testing::{LINEAR, RED, TestResult, TestUi, TestUiBuilder};
use rouge_core::{
    Color, Key, KeyEvent, Point, Pose, Rect, Styled, View, div, dynamic, each, editable,
    focus_handle, overlay, show,
};
use rouge_reactive::{ReadSignal, Runtime, signal};

#[test]
fn shows_inside_each_rows_inside_a_show_keep_their_order_through_reorders() {
    let _rt = Runtime::new().enter();
    let items = signal(vec![1u32, 2, 3]);
    let hidden = signal(0u32);
    let open = signal(true);
    let mut ui = TestUi::new(move || {
        let row = move |id: ReadSignal<u32>| {
            let id = id.get_untracked();
            let width = id as f32 * 10.0;
            let pair = move || vec![div().w(width).h(10.0), div().w(width + 1.0).h(10.0)];
            show(move || hidden.get() != id, pair)
        };
        let rows = move || each(move || items.get(), |id| *id, row);
        div()
            .flex()
            .size_full()
            .child(div().w(5.0).h(10.0))
            .child(show(move || open.get(), rows))
            .child(div().w(7.0).h(10.0))
    });
    assert_eq!(
        row_widths(&mut ui),
        [5.0, 10.0, 11.0, 20.0, 21.0, 30.0, 31.0, 7.0]
    );
    hidden.set(2);
    ui.update();
    assert_eq!(row_widths(&mut ui), [5.0, 10.0, 11.0, 30.0, 31.0, 7.0]);
    items.set(vec![3, 2, 1]);
    ui.update();
    assert_eq!(
        row_widths(&mut ui),
        [5.0, 30.0, 31.0, 10.0, 11.0, 7.0],
        "reordered around the hidden row"
    );
    hidden.set(0);
    ui.update();
    assert_eq!(
        row_widths(&mut ui),
        [5.0, 30.0, 31.0, 20.0, 21.0, 10.0, 11.0, 7.0],
        "the hidden row reappears in its new place"
    );
    items.set(vec![4, 2]);
    ui.update();
    assert_eq!(row_widths(&mut ui), [5.0, 40.0, 41.0, 20.0, 21.0, 7.0]);
    open.set(false);
    ui.update();
    assert_eq!(row_widths(&mut ui), [5.0, 7.0]);
    assert_eq!(ui.stats().elements, 4, "root, container and two boxes");
    open.set(true);
    ui.update();
    assert_eq!(row_widths(&mut ui), [5.0, 40.0, 41.0, 20.0, 21.0, 7.0]);
}

#[test]
fn a_row_removed_from_an_each_inside_a_show_animates_out_while_the_rest_close_the_gap() -> TestResult
{
    let _rt = Runtime::new().enter();
    let items = signal(vec![1u32, 2]);
    let mut ui = TestUi::new(move || {
        let row = |id: ReadSignal<u32>| {
            let color = if id.get_untracked() == 1 {
                Color::RED
            } else {
                Color::BLUE
            };
            div().w(20.0).h(10.0).bg(color)
        };
        let rows = move || {
            each(move || items.get(), |id| *id, row).exit(Pose::new().translate_x(100.0), LINEAR)
        };
        div()
            .flex()
            .size_full()
            .child(show(|| true, rows))
            .child(div().w(30.0).h(10.0))
    });
    assert_eq!(row_widths(&mut ui), [20.0, 20.0, 30.0]);
    items.set(vec![2]);
    ui.advance(0.0);
    assert_eq!(
        row_widths(&mut ui),
        [20.0, 30.0],
        "the remaining row and the sibling take its place"
    );
    assert_eq!(ui.stats().elements, 4, "root, container, row and sibling");
    assert_eq!(ui.first_x(RED, 5)?, 0, "the removed row is still drawn");
    ui.run(0.5);
    assert_eq!(ui.first_x(RED, 5)?, 50, "running its exit animation");
    ui.run(0.55);
    assert_eq!(ui.first_x(RED, 5)?, 200, "until it is done");
    assert_eq!(ui.next_frame(), Wake::Idle);
    Ok(())
}

#[test]
fn a_focused_field_rebuilt_inside_a_reordered_each_row_keeps_focus_and_its_caret() -> TestResult {
    let _rt = Runtime::new().enter();
    let items = signal(vec![1u32, 2]);
    let version = signal(0u32);
    let value = signal("hello".to_owned());
    let (mut ui, focus) = TestUiBuilder::default().build_with(move || {
        let focus = focus_handle();
        let row = move |id: ReadSignal<u32>| {
            let id = id.get_untracked();
            let field = move || {
                let width = 100.0 + version.get() as f32;
                if id == 1 {
                    editable(value)
                        .track_focus(focus)
                        .w(width)
                        .test_id("field")
                        .into_any()
                } else {
                    div().w(width).h(10.0).into_any()
                }
            };
            show(|| true, move || dynamic(field))
        };
        let rows = each(move || items.get(), |id| *id, row);
        (div().size_full().flex().col().child(rows), focus)
    });
    ui.click_on("field")?;
    ui.press("end");
    ui.press("left");
    version.set(1);
    items.set(vec![2, 1]);
    ui.advance(0.0);
    assert!(focus.is_focused(), "the replacement takes over focus");
    ui.type_text("!");
    assert_eq!(value.get(), "hell!o", "with the caret where it was");
    Ok(())
}

#[test]
fn closing_an_autofocused_overlay_returns_focus_to_the_element_that_had_it() {
    let _rt = Runtime::new().enter();
    let open = signal(false);
    let (mut ui, [trigger, inner]) = TestUiBuilder::default().build_with(move || {
        let (trigger, inner) = (focus_handle(), focus_handle());
        let popup = move || {
            overlay(move || {
                div()
                    .focus_scope()
                    .child(div().track_focus(inner).autofocus())
            })
        };
        let root = div()
            .child(div().track_focus(trigger))
            .child(show(move || open.get(), popup));
        (root, [trigger, inner])
    });
    ui.key_down(KeyEvent::new(Key::Tab));
    assert!(trigger.is_focused());
    open.set(true);
    ui.advance(0.0);
    assert!(inner.is_focused(), "autofocus moves focus into the overlay");
    open.set(false);
    ui.advance(0.0);
    assert!(trigger.is_focused(), "and closing it gives focus back");
}

#[test]
fn a_fragment_root_view_mounts_below_overlays_opened_before_it() -> TestResult {
    let _rt = Runtime::new().enter();
    let visible = signal(false);
    let (mut ui, clicked) = TestUiBuilder::default().build_with(move || {
        let clicked = signal(Vec::new());
        let popup = move || {
            div()
                .absolute()
                .size(20.0)
                .on_click(move |_| clicked.update(|all| all.push("overlay")))
        };
        let page = move || {
            div()
                .size_full()
                .on_click(move |_| clicked.update(|all| all.push("page")))
        };
        let root = vec![
            overlay(popup).into_any(),
            show(move || visible.get(), page).into_any(),
        ];
        (root, clicked)
    });
    ui.click_at(50.0, 50.0);
    assert_eq!(clicked.get(), [] as [&str; 0]);
    visible.set(true);
    ui.advance(0.0);
    ui.click_at(10.0, 10.0);
    ui.click_at(50.0, 50.0);
    assert_eq!(clicked.get(), ["overlay", "page"]);
    assert_eq!(ui.bounds(50.0, 50.0)?, Rect::new(0.0, 0.0, 200.0, 100.0));
    Ok(())
}

#[test]
fn an_empty_root_view_mounts_nothing() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| ());
    assert_eq!(ui.stats().elements, 1, "only the window root");
    assert!(!ui.focus_next(false));
    assert!(ui.bounds_at(Point::new(10.0, 10.0)).is_some());
}

#[test]
fn a_parent_whose_only_child_is_absolutely_positioned_can_lose_and_regain_it() -> TestResult {
    let _rt = Runtime::new().enter();
    let visible = signal(true);
    let mut ui = TestUi::new(move || {
        let child = || div().absolute().left(10.0).top(10.0).size(20.0);
        div()
            .relative()
            .size(100.0)
            .child(show(move || visible.get(), child))
    });
    assert_eq!(ui.bounds(15.0, 15.0)?, Rect::new(10.0, 10.0, 20.0, 20.0));
    visible.set(false);
    ui.advance(0.0);
    assert_eq!(ui.bounds(15.0, 15.0)?, Rect::new(0.0, 0.0, 100.0, 100.0));
    visible.set(true);
    ui.advance(0.0);
    assert_eq!(ui.bounds(15.0, 15.0)?, Rect::new(10.0, 10.0, 20.0, 20.0));
    Ok(())
}

fn row_widths(ui: &mut TestUi) -> Vec<f32> {
    let mut widths = Vec::new();
    let mut x = 0.5;
    while let Some(b) = ui
        .bounds_at(Point::new(x, 5.0))
        .filter(|b| b.size.height == 10.0)
    {
        widths.push(b.size.width);
        x = b.origin.x + b.size.width + 0.5;
    }
    widths
}
