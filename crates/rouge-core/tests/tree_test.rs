use rouge_core::testing::{BLACK, BLUE, GREEN, RED, TestResult, TestUi, WHITE, counter};
use rouge_core::{Color, Div, Glass, Point, Spring, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_render::Item;

#[test]
fn elements_get_layers_only_when_they_use_compositor_features() {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        div()
            .size_full()
            .child(div().size(10.0).bg(Color::RED))
            .child(div().size(10.0).opacity(0.5))
            .child(div().size(10.0).layer())
            .child(div().size(10.0).transition(Spring::snappy()))
    });
    assert_eq!(
        ui.stats().layers,
        4,
        "the root plus three promoted elements"
    );
}

#[test]
fn compositor_properties_move_content_and_hit_testing_without_layout_or_paint() -> TestResult {
    let _rt = Runtime::new().enter();
    let x = signal(0.0f32);
    let (clicks, click) = counter();
    let mut ui = TestUi::canvas(|| {
        let target = div().size(20.0).bg(Color::RED).translate_x(x);
        div().size_full().child(target.on_click(move |_| click()))
    });
    assert_eq!(ui.pixel(10, 10)?, RED);
    ui.click(Point::new(10.0, 10.0));
    assert_eq!(clicks.get(), 1);
    let stats = ui.stats();

    x.set(100.0);
    assert!(ui.needs_frame());
    assert!(ui.update_at(0.1));
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "no layout pass, no paint pass and no layer repainted"
    );
    let img = ui.render()?;
    assert_eq!(img.pixel(10, 10), WHITE, "moved away");
    assert_eq!(img.pixel(110, 10), RED, "moved here");

    ui.click(Point::new(10.0, 10.0));
    assert_eq!(clicks.get(), 1, "nothing to hit at the old position");
    ui.click(Point::new(110.0, 10.0));
    assert_eq!(clicks.get(), 2, "hit at the new position");

    x.set(100.0);
    assert!(!ui.needs_frame(), "same value: nothing to do");
    Ok(())
}

#[test]
fn transforms_compose_through_nested_layers() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        let inner = div()
            .size(20.0)
            .bg(Color::BLUE)
            .translate_y(30.0)
            .opacity(0.5);
        div()
            .size_full()
            .child(div().size(40.0).translate_x(50.0).opacity(0.5).child(inner))
    });
    let img = ui.render()?;
    assert_eq!(img.pixel(5, 5), WHITE);
    assert_eq!(
        img.pixel(55, 35),
        [191, 191, 255, 255],
        "both offsets, a quarter opaque over white"
    );
    Ok(())
}

#[test]
fn scale_and_rotation_pivot_on_the_element_center() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        let square = div()
            .absolute()
            .left(40.0)
            .top(20.0)
            .size(40.0)
            .bg(Color::RED);
        let bar = div()
            .absolute()
            .left(120.0)
            .top(20.0)
            .w(60.0)
            .h(20.0)
            .bg(Color::BLUE);
        div()
            .size_full()
            .child(square.scale(0.5))
            .child(bar.rotate(90.0))
    });
    let img = ui.render()?;
    assert_eq!(img.pixel(60, 40), RED, "center unchanged");
    assert_eq!(img.pixel(52, 32), RED, "inside the half-size square");
    assert_eq!(img.pixel(45, 25), WHITE, "outside it");
    assert_eq!(
        img.pixel(150, 5),
        BLUE,
        "the upright bar extends above its layout box"
    );
    assert_eq!(img.pixel(150, 55), BLUE, "and below");
    assert_eq!(img.pixel(125, 30), WHITE, "no longer extends sideways");
    Ok(())
}

#[test]
fn paint_changes_repaint_only_the_layer_they_are_in() -> TestResult {
    let _rt = Runtime::new().enter();
    let inner = signal(Color::RED);
    let outer = signal(Color::WHITE);
    let mut ui = TestUi::canvas(|| {
        div()
            .size_full()
            .bg(outer)
            .child(div().size(20.0).layer().child(div().size(10.0).bg(inner)))
            .child(div().size(20.0).layer().bg(Color::GREEN))
    });
    let before = ui.stats();
    assert_eq!(
        (before.layers, before.layer_paints),
        (3, 3),
        "everything painted once"
    );

    inner.set(Color::BLUE);
    ui.update_at(0.1);
    assert_eq!(
        ui.stats().layer_paints,
        4,
        "only the layer containing the change"
    );
    assert_eq!(ui.pixel(5, 5)?, BLUE);

    outer.set(Color::BLACK);
    ui.update_at(0.2);
    assert_eq!(ui.stats().layer_paints, 5, "only the root layer");
    let img = ui.render()?;
    assert_eq!(img.pixel(150, 80), BLACK);
    assert_eq!(img.pixel(5, 5), BLUE, "nested layers kept their content");
    assert_eq!(img.pixel(5, 25), GREEN);
    Ok(())
}

#[test]
fn content_after_a_nested_layer_composites_above_it() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        div()
            .size_full()
            .child(div().absolute().size(60.0).bg(Color::RED))
            .child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(20.0)
                    .size(60.0)
                    .bg(Color::GREEN)
                    .layer(),
            )
            .child(
                div()
                    .absolute()
                    .left(40.0)
                    .top(40.0)
                    .size(60.0)
                    .bg(Color::BLUE),
            )
    });
    let img = ui.render()?;
    assert_eq!(img.pixel(10, 10), RED);
    assert_eq!(img.pixel(30, 30), GREEN, "layer above earlier sibling");
    assert_eq!(
        img.pixel(50, 50),
        BLUE,
        "later sibling of the parent layer above the layer"
    );
    assert_eq!(
        ui.frame().scene.len(),
        3,
        "root is split into two segments around the layer"
    );
    Ok(())
}

#[test]
fn invisible_layers_are_skipped_and_not_hittable() -> TestResult {
    let _rt = Runtime::new().enter();
    let opacity = signal(0.0f32);
    let clicks = signal(0);
    let mut ui = TestUi::canvas(|| {
        let target = div().size(20.0).bg(Color::RED).opacity(opacity);
        div()
            .size_full()
            .bg(Color::WHITE)
            .child(target.on_click(move |_| clicks.update(|c| *c += 1)))
    });
    assert_eq!(ui.pixel(10, 10)?, WHITE);
    assert_eq!(
        ui.frame().scene.len(),
        1,
        "only the root's background is composited"
    );
    ui.click(Point::new(10.0, 10.0));
    assert_eq!(clicks.get_untracked(), 0);

    opacity.set(1.0);
    ui.update_at(0.1);
    assert_eq!(ui.pixel(10, 10)?, RED);
    ui.click(Point::new(10.0, 10.0));
    assert_eq!(clicks.get_untracked(), 1);
    Ok(())
}

#[test]
fn backdrop_blurs_content_behind_the_element_and_keeps_children_sharp() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        let glass = div()
            .absolute()
            .left(50.0)
            .top(10.0)
            .w(100.0)
            .h(60.0)
            .rounded(12.0);
        let child = div()
            .absolute()
            .left(40.0)
            .top(20.0)
            .size(20.0)
            .bg(Color::RED);
        split_background().child(glass.backdrop(Glass::frosted(8.0)).child(child))
    });
    assert_eq!(ui.stats().layers, 2);
    let img = ui.render()?;
    let edge = img.pixel(100, 20);
    assert!(
        edge[0] > 60 && edge[0] < 200,
        "edge is blurred under the glass: {edge:?}"
    );
    assert_eq!(img.pixel(99, 90)[0], 0, "and sharp outside it");
    assert_eq!(img.pixel(100, 90)[0], 255);
    assert_eq!(
        img.pixel(100, 40),
        RED,
        "the glass's child is drawn on top, unblurred"
    );
    Ok(())
}

#[test]
fn glass_on_an_element_that_paints_nothing_moves_with_its_layer_transform() -> TestResult {
    let _rt = Runtime::new().enter();
    let x = signal(0.0f32);
    let mut ui = TestUi::canvas(|| {
        let glass = div()
            .absolute()
            .w(40.0)
            .h(100.0)
            .backdrop(Glass::frosted(8.0));
        split_background().child(glass.translate_x(x))
    });
    let backdrops = ui
        .frame()
        .scene
        .iter()
        .filter(|item| matches!(item, Item::Layer(layer) if layer.backdrop.is_some()))
        .count();
    assert_eq!(backdrops, 1, "painting nothing still gets a backdrop");
    assert_eq!(ui.pixel(99, 50)?[0], 0, "glass is elsewhere: edge is sharp");
    let stats = ui.stats();
    x.set(80.0);
    ui.update_at(0.1);
    let edge = ui.pixel(100, 50)?[0];
    assert!(
        edge > 60 && edge < 200,
        "glass moved over the edge and blurs it: {edge}"
    );
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "moving glass is compositor-only"
    );
    Ok(())
}

#[test]
fn nested_layers_are_clipped_by_an_ancestor_that_hides_overflow() -> TestResult {
    let _rt = Runtime::new().enter();
    let x = signal(0.0f32);
    let mut ui = TestUi::canvas(|| {
        let wide = div().w(80.0).h(20.0).bg(Color::RED).translate_x(x);
        let window = div()
            .absolute()
            .left(10.0)
            .top(10.0)
            .size(40.0)
            .overflow_hidden();
        div().size_full().child(window.child(wide))
    });
    let img = ui.render()?;
    assert_eq!((img.pixel(20, 20), img.pixel(60, 20)), (RED, WHITE));
    x.set(-60.0);
    ui.update_at(0.1);
    let img = ui.render()?;
    assert_eq!(
        (img.pixel(5, 20), img.pixel(20, 20), img.pixel(35, 20)),
        (WHITE, RED, WHITE)
    );
    Ok(())
}

fn split_background() -> Div {
    div()
        .size_full()
        .bg(Color::WHITE)
        .child(div().absolute().w(100.0).h(100.0).bg(Color::BLACK))
}
