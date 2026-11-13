use rouge_core::testing::{
    BLACK, BLUE, LINEAR, RED, TestError, TestResult, TestUi, TestUiBuilder, WHITE, assert_near,
    counter, recorder,
};
use rouge_core::{
    BackdropBlur, BlendMode, Blur, Color, ColorMatrix, Cursor, Div, Keyframes, LayoutTransition,
    Mask, Point, Pose, Rect, SpanStyle, Spring, Styled, Timing, Underline, View, div, dynamic,
    rich_text, show, text,
};
use rouge_reactive::{Runtime, Signal, signal, stored};
use rouge_render::headless::Image;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn text_content_accepts_strings_closures_and_signals() {
    let _rt = Runtime::new().enter();
    let name = signal(String::from("signal"));
    let n = signal(1);
    let mut ui = build(1.0, || {
        div()
            .child(text("literal"))
            .child(text(String::from("owned")))
            .child(text(name))
            .child(text(move || format!("count {}", n.get())))
            .child(text(move || "static str from closure"))
    });
    assert_eq!(ui.stats().elements, 7);
}

#[test]
fn text_is_measured_by_layout() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = build(1.0, || {
        div()
            .flex()
            .size_full()
            .items_start()
            .child(text("Hello, world").text_size(20.0))
            .child(div().size(10.0))
    });
    let b = ui.bounds(5.0, 5.0)?;
    assert!(
        b.size.width > 60.0 && b.size.width < 200.0,
        "hugs its content: {b:?}"
    );
    assert_eq!(b.size.height, 28.0, "20px at 1.4 line height");
    assert_eq!(
        ui.bounds(b.size.width + 5.0, 5.0)?.origin.x,
        b.size.width,
        "sibling follows"
    );
    Ok(())
}

#[test]
fn text_wraps_to_its_container() -> TestResult {
    let _rt = Runtime::new().enter();
    let sentence = "a reasonably long sentence that cannot possibly fit on a single line";
    let mut ui = build(1.0, || div().w(120.0).child(text(sentence)));
    let b = ui.bounds(5.0, 5.0)?;
    assert_eq!(b.size.width, 120.0);
    let lines = b.size.height / (14.0 * 1.4);
    assert!(lines >= 3.0, "wrapped onto several lines, got {lines}");
    Ok(())
}

#[test]
fn no_wrap_text_keeps_one_line() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = build(1.0, || {
        div()
            .w(60.0)
            .child(text("several words that would wrap").no_wrap())
    });
    assert_eq!(ui.bounds(5.0, 5.0)?.size.height, 20.0);
    Ok(())
}

#[test]
fn content_change_relayouts_but_color_change_only_repaints() -> TestResult {
    let _rt = Runtime::new().enter();
    let label = signal(String::from("short"));
    let color = signal(Color::BLACK);
    let mut ui = build(1.0, || {
        div()
            .flex()
            .size_full()
            .items_start()
            .child(text(label).color(color))
    });
    let before = ui.bounds(2.0, 2.0)?.size.width;
    assert_eq!(ui.passes(), (1, 1));

    color.set(Color::RED);
    ui.update();
    assert_eq!(ui.passes(), (1, 2), "color is paint-only");

    label.set("a considerably longer label".into());
    ui.update();
    assert_eq!(ui.stats().layouts, 2);
    assert!(ui.bounds(2.0, 2.0)?.size.width > before * 2.0);

    label.set("a considerably longer label".into());
    assert!(!ui.needs_frame(), "identical content changes nothing");
    Ok(())
}

#[test]
fn text_draws_colored_glyphs_inside_its_bounds() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = build(1.0, || {
        div()
            .size_full()
            .p(20.0)
            .child(text("Rouge").text_size(40.0).bold().color(Color::RED))
    });
    let b = ui.bounds(25.0, 30.0)?;
    let img = ui.render()?;
    let ink = inked(&img, b);
    assert!(ink > 400, "glyphs cover a fair number of pixels, got {ink}");
    let everywhere = inked(&img, Rect::new(0.0, 0.0, 300.0, 200.0));
    assert_eq!(
        everywhere, ink,
        "nothing is drawn outside the text's bounds"
    );

    let mut solid = 0;
    for y in b.origin.y as u32..(b.origin.y + b.size.height) as u32 {
        for x in b.origin.x as u32..(b.origin.x + b.size.width) as u32 {
            let [r, g, bl, _] = img.pixel(x, y);
            assert_eq!(r, 255, "red channel is saturated everywhere over white");
            assert_eq!(g, bl, "only red is tinted");
            solid += usize::from(g == 0);
        }
    }
    assert!(solid > 100, "stems have fully covered pixels, got {solid}");
    Ok(())
}

#[test]
fn text_is_rasterized_at_device_resolution() -> TestResult {
    let _rt = Runtime::new().enter();
    let make = |scale| {
        build(scale, || {
            div().size_full().child(text("Sharp").text_size(30.0))
        })
    };
    let (mut low, mut high) = (make(1.0), make(2.0));
    let ink_low = inked(&low.render()?, Rect::new(0.0, 0.0, 300.0, 200.0));
    let ink_high = inked(&high.render()?, Rect::new(0.0, 0.0, 600.0, 400.0));
    let ratio = ink_high as f32 / ink_low as f32;
    assert!(
        (3.0..5.0).contains(&ratio),
        "about 4x the pixels at 2x scale, got {ratio}"
    );
    assert_eq!(
        low.bounds(5.0, 5.0)?,
        high.bounds(5.0, 5.0)?,
        "layout ignores the scale"
    );
    Ok(())
}

#[test]
fn text_respects_clipping_and_padding() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = build(1.0, || {
        let clipped = text("clipped text that runs far past its container").no_wrap();
        let container = div().w(40.0).h(30.0).overflow_hidden();
        div()
            .size_full()
            .child(container.child(clipped.pl(10.0).text_size(20.0)))
    });
    let img = ui.render()?;
    assert!(
        inked(&img, Rect::new(10.0, 0.0, 30.0, 30.0)) > 20,
        "visible part is drawn"
    );
    assert_eq!(
        inked(&img, Rect::new(0.0, 0.0, 9.0, 30.0)),
        0,
        "left padding is empty"
    );
    assert_eq!(
        inked(&img, Rect::new(40.0, 0.0, 260.0, 200.0)),
        0,
        "overflow is clipped"
    );
    Ok(())
}

#[test]
fn initial_values_do_not_animate() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        let moved = div().size(20.0).bg(Color::RED).translate_x(move || 50.0);
        div().size_full().child(moved.transition(Spring::smooth()))
    });
    assert!(!ui.is_animating());
    assert_eq!(ui.first_x(RED, 10)?, 50);
    Ok(())
}

#[test]
fn transition_runs_in_the_compositor_and_lands_on_target() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, open, binding_runs) = animated_box(LINEAR);
    assert_eq!(binding_runs.get(), 1);
    let stats = ui.stats();

    open.set(true);
    assert_eq!(
        binding_runs.get(),
        2,
        "the binding ran once to hand over the target"
    );
    assert!(ui.is_animating() && ui.needs_frame());

    ui.update_at(5.0);
    assert_eq!(
        ui.first_x(RED, 10)?,
        0,
        "the animation starts at the first update after the change"
    );
    let mut positions = Vec::new();
    for frame in 1..=20 {
        assert!(ui.update_at(5.0 + f64::from(frame) * 0.05));
        positions.push(ui.first_x(RED, 10)?);
    }
    assert_eq!(positions[1], 10, "linear: 10% after 0.1s");
    assert_eq!(positions[9], 50, "halfway after 0.5s");
    assert!(
        positions.windows(2).all(|w| w[0] <= w[1]),
        "monotonic: {positions:?}"
    );
    assert_eq!(positions[19], 100, "lands exactly on target");
    assert!(!ui.is_animating());
    assert!(!ui.needs_frame(), "idle again once settled");

    assert_eq!(binding_runs.get(), 2, "no reactive work per frame");
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "no layout or paint during the animation"
    );
    Ok(())
}

#[test]
fn spring_transition_can_be_reversed_mid_flight() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, open, _) = animated_box(Spring::smooth());
    open.set(true);
    let frames = run(&mut ui, 8, 60.0)?;
    let mid = frames[7];
    assert!(mid > 5 && mid < 95, "in flight at {mid}");

    open.set(false);
    let after = run(&mut ui, 1, 60.0)?[0];
    assert!(
        after >= mid,
        "momentum carries it on briefly: {mid} -> {after}"
    );
    run(&mut ui, 600, 60.0)?;
    assert!(!ui.is_animating(), "settles");
    assert_eq!(ui.first_x(RED, 10)?, 0, "back at the start");
    Ok(())
}

#[test]
fn springs_settle_on_target_with_or_without_overshoot() -> TestResult {
    let _rt = Runtime::new().enter();
    for (spring, overshoots) in [(Spring::snappy(), false), (Spring::bouncy(), true)] {
        let (mut ui, open, _) = animated_box(spring);
        open.set(true);
        let frames = run(&mut ui, 2000, 120.0)?;
        let peak = frames.iter().max().copied();
        assert!(
            if overshoots {
                peak > Some(105)
            } else {
                peak == Some(100)
            },
            "passes the target only when bouncy: {frames:?}"
        );
        assert_eq!(frames.last(), Some(&100));
        assert!(
            overshoots || (21..240).contains(&frames.len()),
            "a fraction of a second: {}",
            frames.len()
        );
    }
    Ok(())
}

#[test]
fn spring_result_is_frame_rate_independent() -> TestResult {
    let _rt = Runtime::new().enter();
    let at = |hz: f64| {
        let (mut ui, open, _) = animated_box(Spring::smooth());
        open.set(true);
        let frames = run(&mut ui, (0.15 * hz) as usize + 1, hz)?;
        Ok::<_, TestError>(frames[frames.len() - 1].cast_signed())
    };
    let (slow, fast) = (at(60.0)?, at(120.0)?);
    assert!((slow - fast).abs() <= 5, "{slow} vs {fast}");
    Ok(())
}

#[test]
fn long_gaps_do_not_cause_jumps() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, open, _) = animated_box(LINEAR);
    open.set(true);
    ui.update_at(1.0);
    ui.update_at(31.0);
    let x = ui.first_x(RED, 10)?;
    assert!(x < 10, "a 30s stall advances at most one capped tick: {x}");
    Ok(())
}

#[test]
fn removing_an_animating_layer_stops_its_animation() -> TestResult {
    let _rt = Runtime::new().enter();
    let visible = signal(true);
    let x = signal(0.0f32);
    let mut ui = TestUi::canvas(|| {
        let moving = move || {
            div()
                .size(20.0)
                .bg(Color::RED)
                .translate_x(x)
                .transition(Spring::smooth())
        };
        div().size_full().child(show(move || visible.get(), moving))
    });
    assert_eq!(ui.stats().layers, 2);
    x.set(100.0);
    ui.update_at(0.1);
    assert!(ui.is_animating());
    visible.set(false);
    ui.update_at(0.2);
    assert!(!ui.is_animating());
    assert_eq!(
        ui.stats().layers,
        1,
        "the layer and its draw lists were released"
    );
    assert_eq!(ui.pixel(10, 10)?, WHITE);
    Ok(())
}

#[test]
fn animation_frames_upload_no_instance_data() -> TestResult {
    let _rt = Runtime::new().enter();
    let open = signal(false);
    let mut ui = TestUi::canvas(|| {
        let moving = move |i| {
            div()
                .absolute()
                .top(i as f32 * 2.0)
                .size(10.0)
                .bg(Color::RED)
                .translate_x(move || if open.get() { 150.0 } else { 0.0 })
                .rotate(move || if open.get() { 180.0 } else { 0.0 })
                .transition(Spring::bouncy())
        };
        div()
            .size_full()
            .bg(Color::WHITE)
            .child(text("Static content in the root layer"))
            .children((0..50).map(moving))
    });
    let first = ui.draw()?;
    assert!(first.instance_bytes_uploaded > 0);
    assert_eq!(first.layers, 51);

    open.set(true);
    let stats = ui.stats();
    let mut frames = 0;
    while ui.needs_frame() {
        frames += 1;
        ui.update_at(f64::from(frames) / 120.0);
        assert_eq!(
            ui.draw()?.instance_bytes_uploaded,
            0,
            "frame {frames} uploaded instance data"
        );
        assert!(frames < 2000, "animation must settle");
    }
    assert!(frames > 30, "the animation spanned many frames ({frames})");
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "and none of them ran layout or paint"
    );
    Ok(())
}

#[test]
fn keyframes_play_in_the_compositor_then_hold_their_last_pose_or_repeat() -> TestResult {
    let _rt = Runtime::new().enter();
    let play = |keyframes: Keyframes, frames| {
        let mut ui = TestUi::canvas(|| {
            div()
                .size_full()
                .child(div().size(20.0).bg(Color::RED).animate(keyframes))
        });
        let stats = ui.stats();
        assert!(ui.is_animating(), "declared keyframes start on their own");
        let positions = run(&mut ui, frames, 20.0)?;
        let work = ui.stats() - stats;
        assert_eq!(
            (work.layouts, work.paints, work.layer_paints),
            (0, 0, 0),
            "no layout or paint"
        );
        Ok::<_, TestError>((positions, ui.needs_frame()))
    };
    let slide = Keyframes::new()
        .at(0.0, |pose| pose.translate_x(0.0))
        .at(0.5, |pose| pose.translate_x(100.0))
        .at(1.0, |pose| pose.translate_x(40.0))
        .duration(2.0);
    let (positions, awake) = play(slide, 1000)?;
    assert_eq!(
        (positions[9], positions[19], positions.last()),
        (50, 100, Some(&40)),
        "halfway to the first stop, at it, then one-shot keyframes fill forwards"
    );
    assert!(
        positions.len() <= 42 && !awake,
        "and stop after their duration: {}",
        positions.len()
    );

    let spin = Keyframes::new()
        .at(0.0, |pose| pose.translate_x(0.0))
        .at(1.0, |pose| pose.translate_x(100.0))
        .repeat();
    let (positions, _) = play(spin, 50)?;
    assert_eq!(
        (positions.len(), positions[9], positions[29]),
        (50, 50, 50),
        "never settles, and is at the same phase one loop later"
    );
    Ok(())
}

#[test]
fn layout_transitions_animate_position_and_size_with_a_single_layout_pass() -> TestResult {
    let _rt = Runtime::new().enter();
    let pushed = signal(false);
    let mut ui = TestUi::canvas(move || {
        let moved = div()
            .size(20.0)
            .bg(Color::RED)
            .ml(move || if pushed.get() { 100.0 } else { 0.0 })
            .layout_transition(LINEAR);
        let bar = div()
            .w(move || if pushed.get() { 80.0 } else { 40.0 })
            .h(20.0)
            .bg(Color::RED)
            .layout_transition(LayoutTransition::new(LINEAR).size());
        div().size_full().child(moved).child(bar)
    });
    assert!(!ui.is_animating(), "the first layout does not animate");
    let before = ui.stats();

    pushed.set(true);
    ui.advance(0.0);
    assert_eq!(ui.first_x(RED, 10)?, 0, "starts where it was on screen");
    ui.run(0.5);
    let img = ui.render()?;
    assert_eq!(ui.first_x(RED, 10)?, 50);
    assert_eq!(
        (img.pixel(58, 30), img.pixel(62, 30)),
        (RED, WHITE),
        "halfway to its new width"
    );
    ui.run(0.6);
    assert_eq!((ui.first_x(RED, 10)?, ui.pixel(78, 30)?), (100, RED));
    assert_eq!(
        ui.bounds(110.0, 10.0)?.origin.x,
        100.0,
        "layout already has the final position"
    );
    let after = ui.stats();
    assert_eq!(
        after.layouts - before.layouts,
        1,
        "layout ran once, to the final state"
    );
    assert_eq!(after.paints - before.paints, 1);
    Ok(())
}

#[test]
fn moving_a_layer_in_layout_does_not_repaint_it_or_its_parent() -> TestResult {
    let _rt = Runtime::new().enter();
    let pushed = signal(false);
    let mut ui = TestUi::canvas(move || {
        let spacer = div()
            .w(move || if pushed.get() { 100.0 } else { 0.0 })
            .h(10.0);
        let moved = div().size(20.0).bg(Color::RED).layer().child(text("Hi"));
        div()
            .size_full()
            .flex()
            .bg(Color::WHITE)
            .child(spacer)
            .child(moved)
    });
    let before = ui.stats();
    pushed.set(true);
    ui.update();
    assert_eq!(ui.pixel(119, 19)?, RED);
    let after = ui.stats();
    assert_eq!(after.layouts - before.layouts, 1);
    assert_eq!(
        after.layer_paints - before.layer_paints,
        1,
        "only the layer holding the spacer"
    );
    Ok(())
}

#[test]
fn background_and_radius_of_a_layer_animate_without_painting() -> TestResult {
    let _rt = Runtime::new().enter();
    let on = signal(false);
    let bound = move || {
        div()
            .bg(move || if on.get() { Color::BLUE } else { Color::RED })
            .rounded(move || if on.get() { 20.0 } else { 0.0 })
    };
    let (half, done) = fade_to_blue(bound, |_| on.set(true))?;
    assert_eq!(half.pixel(1, 1), WHITE, "the corner is rounding off");
    assert_eq!(done.pixel(5, 5), WHITE);

    let hovered = || div().bg(Color::RED).hover(|s| s.bg(Color::BLUE));
    fade_to_blue(hovered, |ui| ui.pointer_moved(Point::new(20.0, 20.0)))?;
    Ok(())
}

#[test]
fn enter_poses_can_animate_background_and_radius() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        let from = Pose::new().background(Color::BLUE).rounded(20.0);
        let swatch = show(|| true, || div().size(40.0).bg(Color::RED)).enter(from, LINEAR);
        div().size_full().child(swatch)
    });
    let img = ui.render()?;
    assert_eq!(
        (img.pixel(20, 20), img.pixel(3, 3)),
        (BLUE, WHITE),
        "starts from the pose"
    );
    ui.run(1.1);
    let img = ui.render()?;
    assert_eq!(
        (img.pixel(20, 20), img.pixel(3, 3)),
        (RED, RED),
        "and lands on the style"
    );
    Ok(())
}

#[test]
fn effects_masks_and_blend_modes_apply_to_their_element() -> TestResult {
    let _rt = Runtime::new().enter();
    let gray = signal(false);
    let mut ui = TestUi::canvas(|| {
        let tile = |x: f32, color| div().absolute().left(x).top(40.0).size(20.0).bg(color);
        let filter = move || {
            if gray.get() {
                ColorMatrix::grayscale()
            } else {
                ColorMatrix::IDENTITY
            }
        };
        let fade = Mask::linear_gradient(Point::new(0.0, 0.0), Point::new(1.0, 0.0));
        div()
            .size_full()
            .bg(Color::WHITE)
            .child(tile(0.0, Color::RED).effect(filter))
            .child(tile(40.0, Color::RED).mask(fade))
            .child(tile(80.0, Color::BLUE).blend(BlendMode::Difference))
            .child(tile(120.0, Color::RED).effect(Blur::new(4.0)))
    });
    let img = ui.render()?;
    assert_eq!(img.pixel(10, 50), RED, "an identity matrix changes nothing");
    assert!(
        img.pixel(41, 50)[1] < 30 && img.pixel(58, 50)[1] > 225,
        "masked from opaque to clear"
    );
    assert_eq!(img.pixel(90, 50), [255, 255, 0, 255], "white minus blue");
    assert!(
        (150..250).contains(&img.pixel(117, 50)[1]),
        "blur bleeds outside the element"
    );
    let stats = ui.stats();

    gray.set(true);
    ui.advance(0.0);
    assert_near(
        ui.pixel(10, 50)?,
        [54, 54, 54, 255],
        "the bound effect followed its signal",
    );
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "changing an effect neither lays out nor paints"
    );
    Ok(())
}

#[test]
fn effect_parameters_follow_the_element_transition() -> TestResult {
    let _rt = Runtime::new().enter();
    let frosted = signal(false);
    let mut ui = TestUi::canvas(|| {
        let panel = div()
            .absolute()
            .left(80.0)
            .size(40.0)
            .backdrop(move || BackdropBlur::new(if frosted.get() { 16.0 } else { 0.0 }))
            .transition(LINEAR);
        let edge = div().absolute().w(100.0).h(100.0).bg(Color::RED);
        div().size_full().child(edge).child(panel)
    });
    let sharp = ui.render()?;
    assert_eq!(
        (sharp.pixel(95, 20), sharp.pixel(105, 20)),
        (RED, WHITE),
        "no blur yet"
    );
    frosted.set(true);
    ui.run(0.5);
    assert!(ui.needs_frame(), "the radius is still animating");
    let partly = ui.pixel(105, 20)?[1];
    ui.run(0.6);
    let fully = ui.pixel(105, 20)?[1];
    assert!(
        partly < 250 && fully < partly,
        "red spreads further as the radius grows: {partly} then {fully}"
    );
    assert!(!ui.needs_frame());
    Ok(())
}

#[test]
fn a_range_colors_only_its_glyphs_and_background_is_behind_them() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        rich_text(
            "MMMM",
            vec![(
                0..2,
                SpanStyle {
                    color: Some(Color::RED),
                    background: Some(Color::BLUE),
                    ..Default::default()
                },
            )],
        )
        .color(Color::BLACK)
        .text_size(32.0)
        .font("monospace")
    });
    let image = ui.render()?;
    let mut colored = 0;
    let mut plain = 0;
    for y in 0..44 {
        for x in 0..80 {
            match image.pixel(x, y) {
                RED => {
                    colored += 1;
                    assert!(x < 39);
                }
                BLACK => {
                    plain += 1;
                    assert!(x >= 38);
                }
                _ => {}
            }
        }
    }
    assert!(colored > 100 && plain > 100);
    assert_eq!(image.pixel(1, 1), BLUE);
    assert_eq!(image.pixel(60, 1), WHITE);
    Ok(())
}

#[test]
fn rich_text_is_one_measured_leaf_and_wraps_like_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        div().w(100.0).child(rich_text(
            "one two three four five six seven eight nine ten",
            vec![(
                4..20,
                SpanStyle {
                    weight: Some(700.0),
                    ..Default::default()
                },
            )],
        ))
    });
    let bounds = ui.bounds(5.0, 5.0)?;
    assert_eq!(bounds.size.width, 100.0);
    assert!(bounds.size.height >= 60.0);
    assert_eq!(ui.stats().elements, 3);
    assert!(
        ui.text_index_at(Point::new(5.0, 30.0))
            .is_some_and(|i| i > 0)
    );
    Ok(())
}

#[test]
fn range_click_and_hover_report_cluster_start_bytes_and_leave() {
    let _rt = Runtime::new().enter();
    let (clicks, click) = recorder();
    let (hovers, hover) = recorder();
    let mut ui = TestUi::canvas(|| {
        rich_text("éx", Vec::new())
            .text_size(32.0)
            .font("monospace")
            .p(10.0)
            .on_range_click(click)
            .on_range_hover(hover)
    });
    for (point, index) in [(Point::new(12.0, 25.0), 0), (Point::new(35.0, 25.0), 2)] {
        assert_eq!(ui.text_index_at(point), Some(index));
        ui.click(point);
    }
    ui.pointer_left();
    assert_eq!(&*clicks.borrow(), &[Some(0), Some(2)]);
    assert!(hovers.borrow().contains(&Some(2)));
    assert_eq!(hovers.borrow().last(), Some(&None));
    assert_eq!(ui.text_index_at(Point::new(150.0, 80.0)), None);
}

#[test]
fn cursor_follows_a_reactive_prop() {
    let _rt = Runtime::new().enter();
    let pointer = signal(false);
    let mut ui = TestUi::canvas(move || {
        div()
            .size(50.0)
            .cursor(move || pointer.get().then_some(Cursor::Pointer))
    });
    ui.pointer_moved(Point::new(10.0, 10.0));
    assert_eq!(ui.cursor(), Cursor::Default);
    pointer.set(true);
    assert_eq!(ui.cursor(), Cursor::Pointer);
}

#[test]
fn click_text_clicks_the_middle_of_the_matching_text() -> TestResult {
    let _rt = Runtime::new().enter();
    let (clicks, click) = recorder();
    let mut ui = TestUi::canvas(|| {
        rich_text("one two", Vec::new())
            .text_size(32.0)
            .font("monospace")
            .p(10.0)
            .on_range_click(click)
    });
    ui.click_text("two")?;
    assert_eq!(&*clicks.borrow(), &[Some(5)]);
    assert!(matches!(ui.click_text("three"), Err(TestError::NoText(_))));
    Ok(())
}

#[test]
fn tabs_advance_to_the_configured_tab_stop_without_shifting_byte_offsets() {
    let _rt = Runtime::new().enter();
    for (tab_size, index) in [(2, Some(1)), (4, Some(0))] {
        let mut ui = TestUi::canvas(move || {
            rich_text("\tx", Vec::new())
                .text_size(32.0)
                .font("monospace")
                .tab_size(tab_size)
        });
        assert_eq!(ui.text_index_at(Point::new(48.0, 15.0)), index);
    }
}

#[test]
fn reactive_spans_update_and_decorations_paint_over_glyphs() -> TestResult {
    let _rt = Runtime::new().enter();
    let spans = signal(vec![(
        0..2,
        SpanStyle {
            underline: Some(Underline {
                thickness: 3.0,
                color: Color::BLUE,
            }),
            strikethrough: Some(true),
            color: Some(Color::RED),
            ..Default::default()
        },
    )]);
    let mut ui = TestUi::canvas(|| rich_text("MM", spans).text_size(32.0));
    let before = ui.render()?;
    assert!((0..45).any(|y| (0..40).filter(|x| before.pixel(*x, y) == BLUE).count() > 30));
    spans.set(vec![(
        0..2,
        SpanStyle {
            color: Some(Color::BLUE),
            ..Default::default()
        },
    )]);
    ui.advance(0.0);
    let after = ui.render()?;
    assert!((0..45).any(|y| (0..50).any(|x| after.pixel(x, y) == BLUE)));
    assert!(!(0..45).any(|y| (0..50).any(|x| after.pixel(x, y) == RED)));
    Ok(())
}

#[test]
fn a_mounted_subtree_applies_its_reactive_props_at_build_and_shapes_each_text_once() -> TestResult {
    let _rt = Runtime::new().enter();
    let shown = signal(false);
    let width = signal(40.0_f32);
    let mut ui = TestUi::new(move || {
        dynamic(move || {
            if !shown.get() {
                return div().into_any();
            }
            (0..20)
                .fold(div(), |parent, depth| {
                    div()
                        .w(move || width.get() + depth as f32)
                        .bg(move || Color::rgb(0x112233))
                        .child(parent)
                        .child(text(format!("label {depth}")))
                })
                .into_any()
        })
    });
    let before = ui.stats();
    shown.set(true);
    ui.advance(0.0);
    let built = ui.stats() - before;
    assert_eq!(built.text.shapes + built.text.cache_hits, 20);
    assert!(
        built.patches <= 2,
        "the 40 reactive props apply at build, not as {} patches",
        built.patches
    );
    assert_eq!(built.passes, 1);
    Ok(())
}

#[test]
fn a_vec_of_views_builds_its_items_as_siblings_without_a_wrapper() -> TestResult {
    let _rt = Runtime::new().enter();
    let visible = signal(true);
    let mut ui = build(1.0, || {
        let pair = move || vec![div().w(10.0).h(10.0), div().w(20.0).h(10.0)];
        div()
            .flex()
            .size_full()
            .child(show(move || visible.get(), pair))
            .child(div().w(30.0).h(10.0))
    });
    assert_eq!(
        ui.bounds(15.0, 5.0)?.origin.x,
        10.0,
        "items follow each other"
    );
    assert_eq!(
        ui.bounds(35.0, 5.0)?.origin.x,
        30.0,
        "and their parent's later children follow them"
    );
    assert_eq!(
        ui.stats().elements,
        7,
        "root, container, show and vec anchors, three boxes"
    );
    visible.set(false);
    ui.update();
    assert_eq!(
        ui.bounds(5.0, 5.0)?.size.width,
        30.0,
        "every item leaves with the vec"
    );
    assert_eq!(ui.stats().elements, 4);
    Ok(())
}

#[test]
fn handlers_run_in_the_scope_that_registered_them() -> TestResult {
    let _rt = Runtime::new().enter();
    let shown = signal(true);
    let made = stored(Vec::new());
    let mut ui = TestUi::new(move || {
        let button = move || {
            div()
                .test_id("button")
                .size(20.0)
                .on_click(move |_| made.update(|made| made.push(signal(0))))
        };
        div().size_full().child(show(move || shown.get(), button))
    });
    ui.click_on("button")?;
    let created = made.with(|made| made[0]);
    assert!(created.is_alive());
    shown.set(false);
    ui.advance(0.0);
    assert!(
        !created.is_alive(),
        "the signal was owned by the button's branch"
    );
    Ok(())
}

fn build<T: View>(scale: f32, root: impl FnOnce() -> T) -> TestUi {
    TestUiBuilder::default()
        .size(300.0, 200.0)
        .scale(scale)
        .clear(Color::WHITE)
        .build(root)
}

fn inked(img: &Image, area: Rect) -> usize {
    let xs = area.origin.x as u32..(area.origin.x + area.size.width) as u32;
    let ys = area.origin.y as u32..(area.origin.y + area.size.height) as u32;
    ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
        .filter(|(x, y)| img.pixel(*x, *y) != WHITE)
        .count()
}

fn animated_box(timing: impl Into<Timing>) -> (TestUi, Signal<bool>, Rc<Cell<u32>>) {
    let (runs, bump) = counter();
    let timing = timing.into();
    let open = signal(false);
    let ui = TestUi::canvas(move || {
        let x = move || {
            bump();
            if open.get() { 100.0 } else { 0.0 }
        };
        div().size_full().child(
            div()
                .size(20.0)
                .bg(Color::RED)
                .translate_x(x)
                .transition(timing),
        )
    });
    (ui, open, runs)
}

fn fade_to_blue(
    swatch: impl FnOnce() -> Div,
    trigger: impl FnOnce(&mut TestUi),
) -> Result<(Image, Image), TestError> {
    let mut ui = TestUi::canvas(|| {
        div()
            .size_full()
            .child(swatch().size(40.0).transition(LINEAR))
    });
    let img = ui.render()?;
    assert_eq!((img.pixel(20, 20), img.pixel(1, 1)), (RED, RED));
    let stats = ui.stats();
    trigger(&mut ui);
    ui.run(0.5);
    let half = ui.render()?;
    assert_near(half.pixel(20, 20), [128, 0, 128, 255], "halfway there");
    ui.run(0.6);
    let done = ui.render()?;
    assert_eq!(done.pixel(20, 20), BLUE);
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "no layout and no paint at any point"
    );
    assert!(!ui.needs_frame());
    Ok((half, done))
}

fn run(ui: &mut TestUi, max_frames: usize, hz: f64) -> Result<Vec<u32>, TestError> {
    let mut positions = Vec::new();
    while positions.len() < max_frames && ui.is_animating() {
        ui.advance(1.0 / hz);
        positions.push(ui.first_x(RED, 10)?);
    }
    Ok(positions)
}
