use super::camera::{Camera, CameraError};
use super::gallery;
use super::touch::{inbox, library};
use rouge::core::view;
use rouge::core::{InputKind, Point, Role, Styled, UiPalette, div};
use rouge::reactive::{Runtime, signal};
use rouge::widgets::testing::component_test;

const SIZE: (f32, f32) = (1296.0, 4000.0);
const SCALE: f32 = 2.0;
const MIDDLE: f32 = 3000.0;

pub(crate) fn snapshot(dir: &str) -> Result<(), CameraError> {
    let _entered = Runtime::new().enter();
    let ui = component_test()
        .size(SIZE.0, SIZE.1)
        .scale(SCALE)
        .build(gallery);
    let mut camera = Camera { ui, dir };
    camera.shot("gallery-light")?;
    camera
        .ui
        .pointer_moved(Point::new(SIZE.0 / 2.0, SIZE.1 / 2.0));
    camera.ui.scroll(Point::new(0.0, -MIDDLE));
    camera.shot("gallery-light-middle")?;
    camera.ui.click_on("menu")?;
    camera.shot("gallery-light-menu")?;
    camera.ui.press("escape");
    camera.ui.hover_on("volume")?;
    camera.shot("gallery-light-slider")?;
    camera.ui.click_on("open-modal")?;
    camera.shot("gallery-light-modal")?;
    camera.ui.press("escape");
    camera.ui.run(1.0);
    camera.ui.click_on("toast-saved")?;
    camera.ui.click_on("toast-undo")?;
    camera.ui.hover_on("toast-1")?;
    camera.shot("gallery-light-toasts")?;
    camera.ui.click_on("toast-1-action")?;
    camera.ui.hover_on("light")?;
    camera.ui.wait_until(|ui| ui.bounds_of("toast-0").is_none());
    camera.ui.run(1.0);
    camera
        .ui
        .pointer_moved(Point::new(SIZE.0 / 2.0, SIZE.1 / 2.0));
    camera.ui.scroll(Point::new(0.0, -SIZE.1));
    camera.shot("gallery-light-end")?;
    camera.ui.scroll(Point::new(0.0, SIZE.1 + MIDDLE));
    camera.ui.run(1.0);
    camera.ui.click_on("dark")?;
    camera.shot("gallery-dark")?;
    camera
        .ui
        .pointer_moved(Point::new(SIZE.0 / 2.0, SIZE.1 / 2.0));
    camera.ui.scroll(Point::new(0.0, -SIZE.1));
    camera.shot("gallery-dark-end")?;
    camera.ui.scroll(Point::new(0.0, SIZE.1));
    camera.ui.run(1.0);
    camera.ui.click_on("accent-rose")?;
    camera.shot("gallery-dark-rose")?;
    let ui = component_test()
        .size(PHONE.0, 8000.0)
        .input(InputKind::Touch)
        .build(gallery);
    Camera { ui, dir }.shot("phone-gallery")?;
    phone(dir, "light", UiPalette::light())?;
    phone(dir, "dark", UiPalette::dark())
}

fn phone(dir: &str, name: &str, palette: UiPalette) -> Result<(), CameraError> {
    let ui = component_test()
        .size(PHONE.0, PHONE.1)
        .scale(SCALE)
        .input(InputKind::Touch)
        .palette(palette)
        .build(|| {
            view! { <div size_full bg=Role::Canvas>{library(signal(String::new()))}</div> }
        });
    let mut camera = Camera { ui, dir };
    camera.shot(&format!("phone-{name}"))?;
    let row = camera.ui.rect_of("track-Brine")?;
    let (from, to) = (row.max().x - 24.0, row.max().x - 240.0);
    camera.ui.swipe_over(
        Point::new(from, row.center().y),
        Point::new(to, row.center().y),
        0.4,
    );
    camera.shot(&format!("phone-{name}-swipe"))?;
    camera.ui.tap_on("open-sheet")?;
    camera.shot(&format!("phone-{name}-sheet"))?;
    let ui = component_test()
        .size(PHONE.0, PHONE.1)
        .scale(SCALE)
        .input(InputKind::Touch)
        .palette(palette)
        .build(|| {
            view! { <div size_full bg=Role::Canvas>{inbox(signal(String::new()))}</div> }
        });
    let mut camera = Camera { ui, dir };
    camera.shot(&format!("phone-{name}-inbox"))?;
    camera.ui.tap_on("open-settings")?;
    camera.shot(&format!("phone-{name}-settings"))
}

const PHONE: (f32, f32) = (390.0, 760.0);
