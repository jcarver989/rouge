mod common;

use common::{builder, line_widths};
use rouge_core::testing::{BLACK, GREEN, RED, TestError, TestResult, TestUi, WHITE};
use rouge_core::{Color, Palette, Point, PointerButton, Styled, UiPalette, div};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::{ResizablePanel, Side, resizable_panel};

#[test]
fn panels_resize_with_correct_direction_clamp_and_reset_on_double_click() -> TestResult {
    for (side, end) in [
        (Side::Left, Point::new(250.0, 50.0)),
        (Side::Right, Point::new(-100.0, 50.0)),
        (Side::Top, Point::new(50.0, 250.0)),
        (Side::Bottom, Point::new(50.0, -100.0)),
    ] {
        let _entered = Runtime::new().enter();
        let (size, seen, resets) = (signal(60.0), signal(0.0), signal(0));
        let mut ui = builder().build(|| {
            div().size_full().child(
                resizable_panel(side, size)
                    .min_size(40.0)
                    .max_size(120.0)
                    .reset_size(50.0)
                    .on_resize(move |size| seen.set(size))
                    .on_reset(move || resets.update(|n| *n += 1))
                    .w(100.0)
                    .h(100.0)
                    .child(div().size_full()),
            )
        });
        ui.drag_on("panel-resize-handle", end)?;
        assert_eq!((size.get(), seen.get()), (120.0, 120.0));
        let point = ui.center_of("panel-resize-handle")?;
        ui.click(point);
        ui.click(point);
        ui.advance(0.0);
        assert_eq!((size.get(), seen.get(), resets.get()), (50.0, 50.0, 1));
        size.set(10.0);
        ui.advance(0.0);
        assert_eq!(size.get(), 40.0);
    }
    Ok(())
}

#[test]
fn the_divider_rests_as_a_hairline_and_thickens_with_accent_on_hover() -> TestResult {
    for (side, edge, inside) in [
        (Side::Left, Point::new(99.0, 50.0), Point::new(97.0, 50.0)),
        (Side::Right, Point::new(0.0, 50.0), Point::new(2.0, 50.0)),
        (Side::Top, Point::new(50.0, 99.0), Point::new(50.0, 97.0)),
        (Side::Bottom, Point::new(50.0, 0.0), Point::new(50.0, 2.0)),
    ] {
        let _entered = Runtime::new().enter();
        let mut ui = render(move || resizable_panel(side, signal(100.0)));
        assert_eq!(
            (pixel(&mut ui, edge)?, pixel(&mut ui, inside)?),
            (RED, WHITE)
        );
        ui.hover_on("panel-resize-handle")?;
        assert!(accented(pixel(&mut ui, edge)?) && accented(pixel(&mut ui, inside)?));
        ui.pointer_left();
        ui.advance(0.0);
        assert_eq!(
            (pixel(&mut ui, edge)?, pixel(&mut ui, inside)?),
            (RED, WHITE)
        );
    }
    Ok(())
}

#[test]
fn the_divider_stays_highlighted_while_dragging_off_the_handle() -> TestResult {
    let _entered = Runtime::new().enter();
    let size = signal(100.0);
    let mut ui = render(move || resizable_panel(Side::Left, size).max_size(120.0));
    let start = ui.center_of("panel-resize-handle")?;
    ui.pointer_moved(start);
    ui.pointer_down(PointerButton::Primary);
    ui.pointer_moved(Point::new(250.0, 150.0));
    ui.advance(0.0);
    assert_eq!(size.get(), 120.0);
    assert!(accented(pixel(&mut ui, Point::new(117.0, 50.0))?));
    ui.pointer_up(PointerButton::Primary);
    ui.advance(0.0);
    assert_eq!(pixel(&mut ui, Point::new(117.0, 50.0))?, WHITE);
    assert_eq!(pixel(&mut ui, Point::new(119.0, 50.0))?, RED);
    Ok(())
}

#[test]
fn a_divider_color_overrides_the_resting_line_reactively() -> TestResult {
    let _entered = Runtime::new().enter();
    let color = signal(Color::GREEN);
    let mut ui = render(move || resizable_panel(Side::Left, signal(100.0)).divider_color(color));
    assert_eq!(pixel(&mut ui, Point::new(99.0, 50.0))?, GREEN);
    color.set(Color::BLACK);
    ui.advance(0.0);
    assert_eq!(pixel(&mut ui, Point::new(99.0, 50.0))?, BLACK);
    ui.hover_on("panel-resize-handle")?;
    assert!(accented(pixel(&mut ui, Point::new(99.0, 50.0))?));
    Ok(())
}

#[test]
fn the_resting_divider_is_whole_device_pixels_at_any_panel_size() -> TestResult {
    let _entered = Runtime::new().enter();
    let size = signal(100.0);
    let mut ui = builder().scale(1.875).palette(palette()).build(move || {
        div().size_full().child(
            resizable_panel(Side::Left, size)
                .h(100.0)
                .child(div().size_full()),
        )
    });
    let mut widths = Vec::new();
    for width in [100.0, 101.0, 102.0, 103.0] {
        size.set(width);
        ui.advance(0.0);
        let img = ui.render()?;
        widths.extend(line_widths((0..img.width).map(|x| img.pixel(x, 90)), RED));
    }
    assert_eq!(widths, vec![2; 4]);
    Ok(())
}

fn render(panel: impl FnOnce() -> ResizablePanel + 'static) -> TestUi {
    builder().palette(palette()).build(|| {
        div()
            .size_full()
            .child(panel().w(100.0).h(100.0).child(div().size_full()))
    })
}

fn palette() -> Palette {
    Palette {
        border: Color::RED,
        accent: Color::BLUE,
        ..UiPalette::light().map(Color::from)
    }
}

fn pixel(ui: &mut TestUi, at: Point) -> Result<[u8; 4], TestError> {
    ui.pixel(at.x as u32, at.y as u32)
}

fn accented([red, green, blue, _]: [u8; 4]) -> bool {
    blue == 255 && red == green && (64..192).contains(&red)
}
