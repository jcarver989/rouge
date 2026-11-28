use rouge_core::Wake;
use rouge_core::testing::{LINEAR, RED, TestResult, TestUi, TestUiBuilder, WHITE};
use rouge_core::view;
use rouge_core::{
    Animation, Color, Decay, DragPhase, Motion, Point, PointerButton, Spring, Styled, div, motion,
    show,
};
use rouge_reactive::{Runtime, signal};
use rouge_render::headless::Image;
use std::future::Future;
use std::pin::{Pin, pin};
use std::task::{Context, Poll, Waker};

#[test]
fn motion_positions_a_layer_and_set_moves_it_without_layout_or_paint() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, x) = moving_box(10.0);
    assert_eq!(ui.first_x(RED, 10)?, 10);
    assert_eq!(x.get(), 10.0);
    let stats = ui.stats();

    x.set(50.0);
    assert_eq!(x.get(), 50.0, "a set is visible immediately");
    assert_eq!(ui.next_frame(), Wake::Now);
    ui.advance(0.1);
    assert_eq!(ui.first_x(RED, 10)?, 50);
    assert_eq!(ui.next_frame(), Wake::Idle, "a set is not an animation");
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "no layout, no paint"
    );
    Ok(())
}

#[test]
fn animate_to_runs_in_the_compositor_and_can_be_sampled_and_awaited() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, x) = moving_box(0.0);
    let stats = ui.stats();
    let mut settled = pin!(x.animate_to(100.0, LINEAR));
    assert_eq!(poll(settled.as_mut()), Poll::Pending);

    ui.run(0.5);
    assert_eq!(ui.first_x(RED, 10)?, 50);
    assert!(
        (x.get() - 50.0).abs() < 0.01,
        "get samples the animated value: {}",
        x.get()
    );
    assert_eq!(poll(settled.as_mut()), Poll::Pending);

    ui.run(0.55);
    assert_eq!(ui.first_x(RED, 10)?, 100);
    assert_eq!(poll(settled.as_mut()), Poll::Ready(true));
    assert_eq!(ui.next_frame(), Wake::Idle, "idle once settled");
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "nothing but compositing happened"
    );
    Ok(())
}

#[test]
fn a_motion_read_with_value_drives_layout_on_every_frame() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, width) = TestUiBuilder::default().size(200.0, 100.0).build_with(|| {
        let width = motion(20.0);
        let bar = view! { <div test_id="bar" h=10.0 w={move || width.value()}/> };
        (bar, width)
    });
    assert_eq!(ui.rect_of("bar")?.size.width, 20.0);
    assert_eq!(ui.next_frame(), Wake::Idle);
    drop(width.animate_to(120.0, LINEAR));
    ui.run(0.5);
    let halfway = ui.rect_of("bar")?.size.width;
    assert!(
        (halfway - 70.0).abs() < 1.0,
        "laid out part of the way: {halfway}"
    );
    assert_eq!(
        ui.next_frame(),
        Wake::Now,
        "nothing draws with this motion, but its reader must see every frame"
    );
    ui.wait_for_idle()?;
    assert_eq!(ui.rect_of("bar")?.size.width, 120.0);
    assert_eq!(ui.next_frame(), Wake::Idle, "idle once settled");
    Ok(())
}

#[test]
fn stop_and_retarget_interrupt_a_running_animation() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, x) = moving_box(0.0);
    let first = x.animate_to(100.0, Spring::smooth());
    ui.advance(0.0);
    ui.advance(0.05);
    ui.advance(0.05);
    let second = x.animate_to(0.0, Spring::smooth());
    ui.advance(0.05);
    assert_eq!(
        poll(pin!(first)),
        Poll::Ready(false),
        "retargeting interrupts"
    );

    let held = x.get();
    assert!(held > 0.0 && held < 100.0, "in flight at {held}");
    x.stop();
    ui.advance(0.05);
    assert_eq!(poll(pin!(second)), Poll::Ready(false), "so does stopping");
    assert_eq!(x.get(), held, "stop holds the current value");
    assert_eq!(ui.next_frame(), Wake::Idle);
    Ok(())
}

#[test]
fn a_mapped_motion_drives_a_second_prop_from_the_same_value() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, y) = TestUiBuilder::default()
        .clear(Color::WHITE)
        .build_with(move || {
            let y = motion(0.0);
            let moving = view! { <div size=20.0 bg=Color::RED translate_x=y/> };
            let fading = view! {
                <div absolute left=0.0 top=40.0 size=20.0 bg=Color::RED opacity=y.map([0.0, 100.0], [1.0, 0.0])/>
            };
            (view! {
                <div size_full>
                    {moving}
                    {fading}
                </div>
            }, y)
        });
    assert_eq!(ui.pixel(10, 50)?, RED, "the start of the range");
    y.set(50.0);
    ui.advance(0.0);
    let half = ui.pixel(10, 50)?;
    assert!(
        half[1].abs_diff(128) <= 2 && half[0] == 255,
        "half way through the range is half opacity: {half:?}"
    );
    y.set(150.0);
    ui.advance(0.0);
    assert_eq!(
        ui.pixel(10, 50)?,
        WHITE,
        "the output is clamped to its range"
    );
    assert_eq!(ui.first_x(RED, 10)?, 150, "the unmapped binding is not");
    Ok(())
}

#[test]
fn fling_to_starts_a_spring_from_the_release_velocity() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, x) = moving_box(50.0);
    let _settled = x.fling_to(50.0, 400.0, Spring::smooth());
    ui.advance(0.0);
    ui.advance(0.05);
    assert!(
        x.get() > 55.0,
        "the value leaves its target in the direction of the velocity: {}",
        x.get()
    );
    ui.run(3.0);
    assert_eq!(x.get(), 50.0, "and settles back on it");
    assert_eq!(ui.next_frame(), Wake::Idle);
    Ok(())
}

#[test]
fn handlers_can_drive_motions() -> TestResult {
    let _rt = Runtime::new().enter();
    let mut ui = TestUi::canvas(|| {
        let x = motion(0.0);
        let target = div().size(20.0).bg(Color::RED).translate_x(x);
        div().size_full().child(target.on_click(move |_| {
            x.animate_to(100.0, LINEAR);
        }))
    });
    ui.click(Point::new(10.0, 10.0));
    assert!(ui.is_animating());
    ui.run(1.25);
    assert_eq!(ui.first_x(RED, 10)?, 100);
    assert!(!ui.is_animating());
    Ok(())
}

#[test]
fn motions_are_released_with_their_scope() -> TestResult {
    let _rt = Runtime::new().enter();
    let visible = signal(true);
    let slot = signal(None);
    let mut ui = TestUi::canvas(move || {
        let branch = move || {
            let x = motion(30.0);
            slot.set(Some(x));
            div().size(20.0).bg(Color::RED).translate_x(x)
        };
        div().size_full().child(show(move || visible.get(), branch))
    });
    let x: Motion = slot.get_untracked().ok_or("the branch was built")?;
    assert_eq!(ui.first_x(RED, 10)?, 30);
    x.animate_to(80.0, LINEAR);
    ui.advance(0.0);
    ui.advance(0.05);
    assert!(ui.is_animating());

    visible.set(false);
    ui.advance(0.05);
    assert!(!ui.is_animating(), "the motion went away with its scope");
    x.set(5.0);
    assert_eq!(poll(pin!(x.animate_to(1.0, LINEAR))), Poll::Ready(false));
    assert_eq!(x.get(), 0.0, "a disposed motion reads as zero");
    assert_eq!(ui.pixel(40, 10)?, WHITE);
    Ok(())
}

#[test]
fn a_drag_moves_a_motion_and_flings_it_with_decay() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, x) = TestUiBuilder::default()
        .clear(Color::WHITE)
        .build_with(move || {
            let x = motion(0.0);
            let card =
                div()
                    .size(20.0)
                    .bg(Color::RED)
                    .translate_x(x)
                    .on_drag(move |drag| match drag.phase {
                        DragPhase::End => drop(x.decay(drag.velocity.x, Decay::new().rate(40.0))),
                        _ => x.set(drag.translation.x),
                    });
            (div().size_full().child(card), x)
        });
    let stats = ui.stats();
    ui.pointer_moved(Point::new(10.0, 10.0));
    ui.pointer_down(PointerButton::Primary);
    for step in 1..=2 {
        ui.advance(0.01);
        ui.pointer_moved(Point::new(10.0 + 20.0 * step as f32, 10.0));
    }
    ui.advance(0.0);
    assert_eq!(ui.first_x(RED, 10)?, 40, "the card follows the pointer");

    ui.pointer_up(PointerButton::Primary);
    assert_eq!(
        ui.next_frame(),
        Wake::Now,
        "released at 2000px/s, it keeps going"
    );
    ui.run(2.0);
    assert!(
        (x.get() - 90.0).abs() < 0.1,
        "coasting 2000/40 further: {}",
        x.get()
    );
    assert_eq!(ui.next_frame(), Wake::Idle);
    let work = ui.stats() - stats;
    assert_eq!(
        (work.layouts, work.paints, work.layer_paints),
        (0, 0, 0),
        "all of it in the compositor"
    );
    Ok(())
}

#[test]
fn the_compositor_keeps_animating_while_the_ui_is_stalled() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, x) = moving_box(0.0);
    let mut arrived = x.animate_to(100.0, LINEAR);
    ui.advance(0.0);
    ui.draw()?;
    let (before, time) = (ui.stats(), ui.time());

    let first = |image: &Image| (0..image.width).find(|x| image.pixel(*x, 10) == RED);
    let quarter = ui.composite(0.25)?;
    let half = ui.composite(0.5)?;
    assert_eq!(
        first(&quarter),
        Some(25),
        "the compositor drew the frame at 0.25s"
    );
    assert_eq!(first(&half), Some(50), "and at 0.5s");
    let stalled = ui.stats() - before;
    assert_eq!(
        (stalled.layouts, stalled.paints, stalled.patches),
        (0, 0, 0),
        "the UI did no work"
    );
    assert_eq!(ui.time(), time, "the UI's clock did not move");
    assert_eq!(poll(Pin::new(&mut arrived)), Poll::Pending);

    ui.advance(1.0);
    assert_eq!(
        poll(Pin::new(&mut arrived)),
        Poll::Ready(true),
        "the animation resolves from the UI's own model when it resumes"
    );
    assert_eq!(x.get(), 100.0);
    assert_eq!(ui.first_x(RED, 10)?, 100, "and both models agree");
    Ok(())
}

fn moving_box(initial: f32) -> (TestUi, Motion) {
    TestUiBuilder::default()
        .clear(Color::WHITE)
        .build_with(move || {
            let x = motion(initial);
            let moving = div().size(20.0).bg(Color::RED).translate_x(x);
            (div().size_full().child(moving), x)
        })
}

fn poll(settled: Pin<&mut Animation>) -> Poll<bool> {
    settled.poll(&mut Context::from_waker(Waker::noop()))
}
