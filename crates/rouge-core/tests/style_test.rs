use rouge_core::testing::{BLUE, GREEN, RED, TestResult, TestUi, TestUiBuilder};
use rouge_core::{Color, Rect, Styled, div, editable, rich_text, text, use_theme};
use rouge_reactive::{Runtime, signal};

#[test]
fn borders_take_layout_space_and_paint() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div()
            .size_full()
            .border(10.0)
            .border_color(Color::BLUE)
            .bg(Color::RED)
            .child(div().size_full().bg(Color::GREEN))
    });
    let img = ui.render()?;
    assert_eq!(img.pixel(5, 50), BLUE, "border");
    assert_eq!(img.pixel(15, 50), GREEN, "child starts inside the border");
    assert_eq!(ui.bounds(100.0, 50.0)?, Rect::new(10.0, 10.0, 180.0, 80.0));
    Ok(())
}

#[test]
fn flex_row_lays_children_out_with_gap_and_padding() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div()
            .flex()
            .size_full()
            .p(10.0)
            .gap(20.0)
            .child(div().w(50.0).h(30.0))
            .child(div().flex_1().h_full())
    });
    assert_eq!(ui.bounds(11.0, 11.0)?, Rect::new(10.0, 10.0, 50.0, 30.0));
    assert_eq!(
        ui.bounds(150.0, 50.0)?,
        Rect::new(80.0, 10.0, 110.0, 80.0),
        "takes the rest"
    );
    Ok(())
}

#[test]
fn block_children_stack_and_column_centering_works() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        let centered = div().flex().col().items_center().justify_center().h(80.0);
        div()
            .size_full()
            .child(div().h(20.0))
            .child(centered.child(div().size(40.0)))
    });
    assert_eq!(ui.bounds(5.0, 5.0)?, Rect::new(0.0, 0.0, 200.0, 20.0));
    assert_eq!(ui.bounds(100.0, 60.0)?, Rect::new(80.0, 40.0, 40.0, 40.0));
    Ok(())
}

#[test]
fn absolute_positioning_uses_insets() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::new(|| {
        div()
            .size_full()
            .child(div().absolute().right(10.0).bottom(5.0).w(30.0).h(20.0))
    });
    assert_eq!(ui.bounds(175.0, 85.0)?, Rect::new(160.0, 75.0, 30.0, 20.0));
    Ok(())
}

#[test]
fn text_inherits_parent_typography_and_local_color_wins() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().size(200.0, 150.0).build(|| {
        div()
            .text_color(Color::RED)
            .text_size(30.0)
            .font("monospace")
            .font_weight(700.0)
            .italic()
            .line_height(1.0)
            .child(text("MMMM"))
            .child(rich_text("MMMM", Vec::new()).color(Color::BLUE))
            .child(editable(signal("MMMM".into())))
    });
    let image = ui.render()?;
    for (y, color) in [(0, RED), (30, BLUE), (60, RED)] {
        assert_eq!(ui.bounds(1.0, y as f32 + 1.0)?.size.height, 30.0);
        assert!((y..y + 30).any(|y| (0..100).any(|x| image.pixel(x, y) == color)));
    }
    Ok(())
}

#[test]
fn inherited_color_changes_only_repaint_inheriting_layers() -> TestResult {
    let _rt = Runtime::new().enter();
    let color = signal(Color::RED);
    let mut ui = TestUi::canvas(|| {
        div()
            .text_color(color)
            .text_size(30.0)
            .child(div().layer().child(text("MMMM")))
            .child(div().layer().child(text("MMMM").color(Color::BLUE)))
    });
    let before = ui.stats();
    color.set(Color::GREEN);
    ui.advance(0.0);
    let after = ui.stats();
    assert_eq!(after.layouts, before.layouts);
    assert_eq!(after.paints, before.paints + 1);
    assert_eq!(after.layer_paints, before.layer_paints + 1);
    let image = ui.render()?;
    assert!((0..42).any(|y| (0..100).any(|x| image.pixel(x, y) == GREEN)));
    assert!((42..84).any(|y| (0..100).any(|x| image.pixel(x, y) == BLUE)));
    Ok(())
}

#[test]
fn inherited_size_changes_relayout_but_an_override_shields_the_leaf() -> TestResult {
    let _rt = Runtime::new().enter();
    let size = signal(20.0);
    let mut ui = TestUi::new(|| div().text_size(size).child(div().child(text("size"))));
    assert_eq!(ui.bounds(1.0, 1.0)?.size.height, 28.0);
    size.set(30.0);
    ui.advance(0.0);
    assert_eq!(ui.bounds(1.0, 1.0)?.size.height, 42.0);
    let mut shielded = TestUi::new(|| div().text_size(size).child(text("size").text_size(14.0)));
    let before = shielded.stats();
    size.set(40.0);
    shielded.advance(0.0);
    assert_eq!(shielded.stats(), before);
    Ok(())
}

#[test]
fn unstyled_leaves_fall_back_to_the_reactive_theme() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, theme) = TestUiBuilder::default().build_with(|| (text("MMMM"), use_theme()));
    let before = ui.stats();
    theme.text.set(Color::RED);
    ui.advance(0.0);
    assert_eq!(ui.stats().layouts, before.layouts);
    let image = ui.render()?;
    assert!((0..20).any(|y| (0..70).any(|x| image.pixel(x, y) == RED)));
    theme.text_size.set(30.0);
    ui.advance(0.0);
    assert_eq!(ui.bounds(1.0, 1.0)?.size.height, 42.0);
    Ok(())
}
