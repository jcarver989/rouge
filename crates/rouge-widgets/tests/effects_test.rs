use rouge_core::testing::{TestResult, TestUi};
use rouge_core::view;
use rouge_core::{Color, Point, PointerButton, Styled, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use rouge_widgets::tokens::Radius;
use rouge_widgets::{BusyExt, ButtonVariant, button};

#[test]
fn a_press_ripple_spreads_from_the_pointer_inside_the_button_and_then_is_gone() -> TestResult {
    let _entered = Runtime::new().enter();
    let mut ui = component_test()
        .size(300.0, 100.0)
        .clear(Color::WHITE)
        .build(|| {
            view! {
                <div p=20.0 flex>
                    <button label="Save changes" test_id="save" variant=ButtonVariant::Primary ripple w=200.0 h=40.0/>
                </div>
            }
        });
    let at = ui.rect_of("save")?;
    let near = Point::new(at.origin.x + 30.0, at.center().y + 12.0);
    let sample = |ui: &mut TestUi, point: Point| ui.pixel(point.x as u32, point.y as u32);
    assert_eq!(
        ui.compositor_frames(1.0)?,
        0,
        "an idle button draws nothing"
    );
    ui.pointer_moved(near);
    ui.run(0.3);
    let rest = sample(&mut ui, near + Point::new(8.0, 0.0))?;
    let outside = sample(&mut ui, Point::new(at.origin.x - 3.0, at.center().y))?;
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_up(PointerButton::Primary);
    ui.advance(0.1);
    assert!(ui.bounds_of("save-ripple").is_some());
    assert_ne!(
        sample(&mut ui, near + Point::new(8.0, 0.0))?,
        rest,
        "ink near the press"
    );
    assert_eq!(
        sample(&mut ui, Point::new(at.max().x - 6.0, at.center().y + 12.0))?,
        rest,
        "which has not reached the far side yet"
    );
    assert_eq!(
        sample(&mut ui, Point::new(at.origin.x - 3.0, at.center().y))?,
        outside,
        "and never leaves the button"
    );
    ui.run(1.0);
    assert!(
        ui.bounds_of("save-ripple").is_none(),
        "the layer is removed"
    );
    assert_eq!(sample(&mut ui, near + Point::new(8.0, 0.0))?, rest);
    assert_eq!(
        ui.compositor_frames(1.0)?,
        0,
        "and the window is idle again"
    );
    Ok(())
}

#[test]
fn a_busy_rim_travels_inside_the_edge_only_while_busy() -> TestResult {
    let _entered = Runtime::new().enter();
    let busy = signal(false);
    let mut ui = component_test()
        .size(200.0, 120.0)
        .clear(Color::WHITE)
        .build(move || {
            view! {
                <div p=20.0>
                    <div test_id="panel" w=160.0 h=80.0 bg=Color::WHITE busy=(busy, Radius::Floating)/>
                </div>
            }
        });
    assert!(ui.bounds_of("busy-rim").is_none());
    assert_eq!(ui.compositor_frames(1.0)?, 0, "idle while not busy");
    busy.set(true);
    ui.advance(0.0);
    let mut lit = Vec::new();
    for _ in 0..14 {
        ui.advance(0.1);
        let image = ui.render()?;
        lit.push(image.pixel(100, 20) != [255; 4]);
        assert_eq!(image.pixel(100, 60), [255; 4], "the middle stays clear");
        assert_eq!(
            image.pixel(100, 17),
            [255; 4],
            "nothing glows outside the edge"
        );
    }
    assert!(
        lit.contains(&true) && lit.contains(&false),
        "the light travels: {lit:?}"
    );
    busy.set(false);
    ui.run(0.5);
    assert!(ui.bounds_of("busy-rim").is_none());
    assert_eq!(ui.pixel(100, 20)?, [255; 4], "the rim is gone");
    assert_eq!(ui.compositor_frames(1.0)?, 0, "idle again");
    Ok(())
}
