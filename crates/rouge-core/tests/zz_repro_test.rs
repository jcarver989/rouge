use rouge_core::Scrollable;
use rouge_core::testing::TestUi;
use rouge_core::{Styled, div, scroll};
use rouge_reactive::{Runtime, Signal, signal};
use rouge_render::{Color, Point, ScrollState};

#[test]
fn repro_macos_flick_into_bottom_edge() {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = scroller(state);
    ui.pointer_moved(Point::new(50.0, 50.0));
    let dt = 1.0 / 60.0;
    for _ in 0..5 {
        ui.scroll_drag(Point::new(0.0, -30.0));
        ui.advance(dt);
    }
    println!(
        "finger up at {:.1} (max 200)",
        state.get_untracked().offset.y
    );
    ui.scroll_release();
    ui.advance(dt);
    let mut delta = 30.0_f32;
    let mut trace = Vec::new();
    while delta > 0.5 {
        ui.scroll_drag(Point::new(0.0, -delta));
        ui.advance(dt);
        trace.push(state.get_untracked().offset.y);
        delta *= 0.95;
    }
    let peak = trace.iter().copied().fold(f32::MIN, f32::max);
    println!(
        "momentum frames {} peak {:.1} at end {:.1}",
        trace.len(),
        peak,
        trace.last().unwrap()
    );
    println!(
        "trace: {:?}",
        trace.iter().map(|v| v.round()).collect::<Vec<_>>()
    );
    println!("needs_frame before release {}", ui.needs_frame());
    ui.scroll_release();
    println!("needs_frame after release {}", ui.needs_frame());
    let mut after = Vec::new();
    for _ in 0..40 {
        ui.advance(dt);
        after.push(state.get_untracked().offset.y.round());
    }
    println!("after release: {after:?}");
}

fn scroller(state: Signal<ScrollState>) -> TestUi {
    TestUi::canvas(move || {
        let container = scroll().w(100.0).h(100.0).scroll_state(state);
        let band = |color| div().h(100.0).bg(color);
        div().size_full().child(
            container.child(
                div()
                    .child(band(Color::RED))
                    .child(band(Color::BLUE))
                    .child(band(Color::GREEN)),
            ),
        )
    })
}

#[test]
fn proposed_release_with_finger_velocity_and_drop_momentum() {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let mut ui = scroller(state);
    ui.pointer_moved(Point::new(50.0, 50.0));
    let dt = 1.0 / 60.0;
    for _ in 0..5 {
        ui.scroll_drag(Point::new(0.0, -30.0));
        ui.advance(dt);
    }
    ui.scroll_release();
    let mut trace = Vec::new();
    for _ in 0..90 {
        ui.advance(dt);
        trace.push(state.get_untracked().offset.y.round());
    }
    let peak = trace.iter().copied().fold(f32::MIN, f32::max);
    let settled = trace.iter().position(|v| *v == 200.0 && true).unwrap_or(0);
    println!(
        "fixed: peak {peak} settled-frame {settled} needs_frame {}",
        ui.needs_frame()
    );
    println!("fixed trace: {trace:?}");
}
