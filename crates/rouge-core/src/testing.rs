use crate::event::{Key, KeyEvent, Modifiers, PointerButton};
use crate::ui::Ui;
use crate::view::View;
use rouge_render::headless::{Headless, Image};
use rouge_render::{Color, Ease, GpuError, Point, Rect, RenderStats, Size, Tween, Viewport};
use std::ops::{Deref, DerefMut};
use std::thread::sleep;
use std::time::Duration;
use thiserror::Error;

pub use rouge_reactive::testing::{TestResult, counter, recorder};
pub use rouge_render::headless::assert_near;

pub const LINEAR: Tween = Tween::new(1.0).ease(Ease::Linear);

#[derive(Debug, Error)]
pub enum TestError {
    #[error("no element at ({x}, {y})")]
    NoElement { x: f32, y: f32 },
    #[error(transparent)]
    Gpu(#[from] GpuError),
}

pub const RED: [u8; 4] = [255, 0, 0, 255];
pub const GREEN: [u8; 4] = [0, 255, 0, 255];
pub const BLUE: [u8; 4] = [0, 0, 255, 255];
pub const WHITE: [u8; 4] = [255, 255, 255, 255];
pub const BLACK: [u8; 4] = [0, 0, 0, 255];
pub const CLEAR: [u8; 4] = [0, 0, 0, 0];

const STEP: f64 = 0.05;

pub struct TestUiBuilder {
    size: Size,
    scale: f32,
    clear: Color,
}

impl Default for TestUiBuilder {
    fn default() -> Self {
        TestUiBuilder {
            size: Size::new(200.0, 100.0),
            scale: 1.0,
            clear: Color::TRANSPARENT,
        }
    }
}

impl TestUiBuilder {
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = Size::new(width, height);
        self
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    pub fn clear(mut self, clear: Color) -> Self {
        self.clear = clear;
        self
    }

    pub fn build<T: View>(self, root: impl FnOnce() -> T) -> TestUi {
        self.build_with(|| (root(), ())).0
    }

    pub fn build_with<T: View, U>(self, root: impl FnOnce() -> (T, U)) -> (TestUi, U) {
        let mut extra = None;
        let mut ui = Ui::new(self.size, || {
            let (view, value) = root();
            extra = Some(value);
            view
        });
        ui.set_scale(self.scale);
        ui.update_at(0.0);
        let ui = TestUi {
            ui,
            size: self.size,
            clear: self.clear,
            headless: None,
            clock: 0.0,
        };
        (ui, extra.expect("the root view was built"))
    }
}

pub struct TestUi {
    ui: Ui,
    size: Size,
    clear: Color,
    headless: Option<Headless>,
    clock: f64,
}

impl TestUi {
    pub fn new<T: View>(root: impl FnOnce() -> T) -> TestUi {
        TestUiBuilder::default().build(root)
    }

    pub fn canvas<T: View>(root: impl FnOnce() -> T) -> TestUi {
        TestUiBuilder::default().clear(Color::WHITE).build(root)
    }

    fn present(&mut self) -> Result<(RenderStats, &Headless), TestError> {
        let scale = self.ui.scale();
        let headless = match self.headless.take() {
            Some(headless) => headless,
            None => Headless::new(Viewport::new(
                (self.size.width * scale) as u32,
                (self.size.height * scale) as u32,
                scale,
            ))?,
        };
        let headless = self.headless.insert(headless);
        let frame = self.ui.frame();
        let stats = headless.render(self.clear, &frame.scene, frame.atlas);
        Ok((stats, headless))
    }

    pub fn draw(&mut self) -> Result<RenderStats, TestError> {
        Ok(self.present()?.0)
    }

    pub fn wait(&self) -> Result<(), TestError> {
        if let Some(headless) = &self.headless {
            headless.wait()?;
        }
        Ok(())
    }

    pub fn render(&mut self) -> Result<Image, TestError> {
        Ok(self.present()?.1.image()?)
    }

    pub fn pixel(&mut self, x: u32, y: u32) -> Result<[u8; 4], TestError> {
        Ok(self.render()?.pixel(x, y))
    }

    pub fn first_x(&mut self, color: [u8; 4], y: u32) -> Result<u32, TestError> {
        let image = self.render()?;
        let found = (0..image.width).find(|x| image.pixel(*x, y) == color);
        Ok(found.unwrap_or(image.width))
    }

    pub fn bounds(&mut self, x: f32, y: f32) -> Result<Rect, TestError> {
        self.ui
            .bounds_at(Point::new(x, y))
            .ok_or(TestError::NoElement { x, y })
    }

    pub fn passes(&mut self) -> (u64, u64) {
        let stats = self.ui.stats();
        (stats.layouts, stats.paints)
    }

    pub fn advance(&mut self, seconds: f64) -> bool {
        self.clock += seconds;
        self.ui.update_at(self.clock)
    }

    pub fn run(&mut self, seconds: f64) {
        self.advance(0.0);
        for _ in 0..(seconds / STEP).round() as usize {
            self.advance(STEP);
        }
    }

    pub fn wait_until(&mut self, mut done: impl FnMut(&mut TestUi) -> bool) -> bool {
        for _ in 0..400 {
            self.advance(STEP);
            if done(self) {
                return true;
            }
            sleep(Duration::from_millis(2));
        }
        false
    }

    pub fn click(&mut self, position: Point) {
        self.ui.pointer_moved(position);
        self.ui.pointer_down(PointerButton::Primary);
        self.ui.pointer_up(PointerButton::Primary);
    }

    pub fn click_at(&mut self, x: f32, y: f32) {
        self.advance(1.0);
        self.click(Point::new(x, y));
        self.advance(0.0);
    }

    pub fn press(&mut self, key: Key) -> bool {
        self.press_with(key, Modifiers::NONE)
    }

    pub fn press_with(&mut self, key: Key, modifiers: Modifiers) -> bool {
        let handled = self
            .ui
            .key_down(KeyEvent::new(key).with_modifiers(modifiers));
        self.advance(0.0);
        handled
    }

    pub fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.ui.key_down(KeyEvent::typed(&character.to_string()));
        }
        self.advance(0.0);
    }
}

impl Deref for TestUi {
    type Target = Ui;

    fn deref(&self) -> &Ui {
        &self.ui
    }
}

impl DerefMut for TestUi {
    fn deref_mut(&mut self) -> &mut Ui {
        &mut self.ui
    }
}
