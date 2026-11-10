#[path = "shared/mod.rs"]
mod shared;

use rouge::AppError;
use rouge::core::testing::{TestError, TestUi, TestUiBuilder};
use rouge::prelude::*;
use rouge::render::GpuError;
use rouge::render::headless::EncodeError;
use shared::counter_view::counter;
use shared::diff::{Patch, SAMPLE, diff_view};
use shared::effects_view::effects_demo;
use shared::glass_view::glass;
use shared::motion_view::motion_demo;
use shared::settings_view::settings;
use std::env::args;
use std::fs::{read_to_string, write};
use std::io::{Error as IoError, read_to_string as read_all, stdin};
use thiserror::Error;

const SCALE: f32 = 2.0;
const DARK: Color = Color::rgb(0x1e1e2e);

#[derive(Debug, Error)]
enum DemoError {
    #[error("failed to read or write a file: {0}")]
    Io(#[from] IoError),
    #[error(transparent)]
    App(#[from] AppError),
    #[error(transparent)]
    Gpu(#[from] GpuError),
    #[error(transparent)]
    Encode(#[from] EncodeError),
    #[error(transparent)]
    Test(#[from] TestError),
}

struct Demo {
    name: &'static str,
    size: (f32, f32),
    background: Color,
    root: fn(Option<&str>) -> AnyView,
    shots: fn(&mut Camera) -> Result<(), DemoError>,
}

const DEMOS: [Demo; 6] = [
    Demo {
        name: "counter",
        size: (560.0, 360.0),
        background: DARK,
        root: |_| counter().into_any(),
        shots: counter_shots,
    },
    Demo {
        name: "glass",
        size: (600.0, 380.0),
        background: Color::rgb(0x10111a),
        root: |_| glass(signal(false)).into_any(),
        shots: glass_shots,
    },
    Demo {
        name: "effects",
        size: (860.0, 300.0),
        background: Color::rgb(0x14151f),
        root: |_| effects_demo(signal(false)).into_any(),
        shots: effects_shots,
    },
    Demo {
        name: "motion",
        size: (760.0, 480.0),
        background: DARK,
        root: |_| motion_demo().into_any(),
        shots: motion_shots,
    },
    Demo {
        name: "diff",
        size: (1280.0, 800.0),
        background: DARK,
        root: |patch| diff_view(Patch::parse(patch.unwrap_or(SAMPLE))),
        shots: diff_shots,
    },
    Demo {
        name: "settings",
        size: (560.0, 520.0),
        background: Color::WHITE,
        root: |_| settings().into_any(),
        shots: settings_shots,
    },
];

fn main() -> Result<(), DemoError> {
    let name = args().nth(1).unwrap_or_default();
    if name == "--snapshot" {
        return snapshot(&args().nth(2).unwrap_or_else(|| ".".into()));
    }
    let Some(demo) = DEMOS.iter().find(|demo| demo.name == name) else {
        let names = DEMOS.map(|demo| demo.name).join(" | ");
        println!("usage: demo <{names}> [patch | -]  or  demo --snapshot [dir]");
        return Ok(());
    };
    let input = match args().nth(2).as_deref() {
        None => None,
        Some("-") => Some(read_all(stdin())?),
        Some(path) => Some(read_to_string(path)?),
    };
    let options = WindowOptions::default()
        .title(format!("Rouge {name}"))
        .size(demo.size.0, demo.size.1)
        .background(demo.background);
    Ok(App::new()
        .window(options, move || (demo.root)(input.as_deref()))
        .run()?)
}

fn snapshot(dir: &str) -> Result<(), DemoError> {
    let _entered = Runtime::new().enter();
    for demo in &DEMOS {
        let ui = TestUiBuilder::default()
            .size(demo.size.0, demo.size.1)
            .scale(SCALE)
            .clear(DARK)
            .build(|| (demo.root)(None));
        (demo.shots)(&mut Camera { ui, dir })?;
    }
    Ok(())
}

struct Camera<'a> {
    ui: TestUi,
    dir: &'a str,
}

impl Camera<'_> {
    fn save(&mut self, name: &str) -> Result<(), DemoError> {
        let path = format!("{}/{name}.png", self.dir);
        write(&path, self.ui.render()?.to_png()?)?;
        println!("wrote {path} ({:?})", self.ui.stats());
        Ok(())
    }

    fn click(&mut self, x: f32, y: f32) {
        self.ui.click(Point::new(x, y));
    }
}

fn counter_shots(camera: &mut Camera) -> Result<(), DemoError> {
    camera.save("counter-0")?;
    for _ in 0..6 {
        camera.click(346.0, 266.0);
    }
    camera.ui.run(0.1);
    camera.save("counter-6")
}

fn glass_shots(camera: &mut Camera) -> Result<(), DemoError> {
    camera.save("glass-rest")?;
    camera.click(300.0, 120.0);
    camera.ui.run(0.1);
    camera.save("glass-springing")
}

fn effects_shots(camera: &mut Camera) -> Result<(), DemoError> {
    camera.save("effects-off")?;
    camera.click(80.0, 200.0);
    camera.ui.pointer_moved(Point::new(300.0, 170.0));
    camera.ui.pointer_down(PointerButton::Primary);
    for step in 1..=10u8 {
        let travel = f32::from(step) * 6.0;
        camera
            .ui
            .pointer_moved(Point::new(300.0 + travel * 2.0, 170.0 - travel));
        camera.ui.advance(0.05);
    }
    camera.ui.pointer_up(PointerButton::Primary);
    camera.save("effects-on")
}

fn motion_shots(camera: &mut Camera) -> Result<(), DemoError> {
    camera.ui.run(1.0);
    camera.save("motion-rest")?;
    camera.ui.pointer_moved(Point::new(200.0, 300.0));
    camera.ui.scroll(Point::new(0.0, -1_400_000.0));
    camera.click(500.0, 64.0);
    camera.click(600.0, 280.0);
    camera.click(600.0, 180.0);
    camera.ui.run(0.1);
    camera.save("motion-moving")
}

fn diff_shots(camera: &mut Camera) -> Result<(), DemoError> {
    camera.save("diff-unified")?;
    camera.ui.pointer_moved(Point::new(700.0, 400.0));
    camera.ui.scroll(Point::new(0.0, -2610.0));
    camera.ui.run(0.1);
    camera.save("diff-scrolled")?;
    camera.click(1212.0, 32.0);
    camera.ui.run(0.5);
    camera.save("diff-split")?;
    camera.click(700.0, 500.0);
    camera.ui.run(0.15);
    camera.save("diff-selected")
}

fn settings_shots(camera: &mut Camera) -> Result<(), DemoError> {
    camera.ui.run(0.5);
    camera.click(400.0, 100.0);
    camera.ui.key_down(KeyEvent::typed("!"));
    camera.ui.run(0.5);
    camera.save("settings-general")?;
    for (name, tab, target) in [
        ("settings-dark", 120.0, (509.0, 107.0)),
        ("settings-modal", 200.0, (493.0, 150.0)),
    ] {
        camera.click(tab, 18.0);
        camera.ui.run(0.5);
        camera.click(target.0, target.1);
        camera.ui.run(0.5);
        camera.save(name)?;
    }
    Ok(())
}
