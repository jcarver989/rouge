use crate::event::{FileDropPhase, Key, KeyEvent, Modifiers, PointerButton};
use crate::ui::Ui;
use crate::view::View;
use crate::window::{Menu, WindowCommand};
use rouge_render::headless::{Headless, Image};
use rouge_render::{Color, Ease, GpuError, Point, Rect, RenderStats, Size, Tween, Viewport};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
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
    #[error("no element with test id {0:?}")]
    NoTestId(String),
    #[error("{0:?} never scrolled into view")]
    NotScrolledIntoView(String),
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
const DRAG_STEPS: u8 = 8;
const MAX_SCROLLS: usize = 100;

pub struct TestUiBuilder {
    menus: Vec<Menu>,
    fonts: Vec<Vec<u8>>,
    size: Size,
    scale: f32,
    clear: Color,
}

impl Default for TestUiBuilder {
    fn default() -> Self {
        TestUiBuilder {
            menus: Vec::new(),
            fonts: Vec::new(),
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

    pub fn fonts(mut self, fonts: impl IntoIterator<Item = Vec<u8>>) -> Self {
        self.fonts.extend(fonts);
        self
    }

    pub fn menus(mut self, menus: Vec<Menu>) -> Self {
        self.menus = menus;
        self
    }

    pub fn build<T: View>(self, root: impl FnOnce() -> T) -> TestUi {
        self.build_with(|| (root(), ())).0
    }

    pub fn build_with<T: View, U>(self, root: impl FnOnce() -> (T, U)) -> (TestUi, U) {
        let mut extra = None;
        let mut ui = Ui::with_fonts(self.size, self.fonts, || {
            let (view, value) = root();
            extra = Some(value);
            view
        });
        ui.set_scale(self.scale);
        ui.bind_menus(&self.menus);
        let mut ui = TestUi {
            ui,
            size: self.size,
            clear: self.clear,
            headless: None,
            clock: 0.0,
            commands: Vec::new(),
        };
        ui.advance(0.0);
        (ui, extra.expect("the root view was built"))
    }
}

pub struct TestUi {
    ui: Ui,
    size: Size,
    clear: Color,
    headless: Option<Headless>,
    clock: f64,
    commands: Vec<WindowCommand>,
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
        let changed = self.ui.update_at(self.clock);
        loop {
            let commands = self.ui.take_window_commands();
            if commands.is_empty() {
                return changed;
            }
            for command in commands {
                match command {
                    WindowCommand::Menu(item) => {
                        item.dispatch(&mut self.ui);
                    }
                    WindowCommand::Focus => self.ui.set_focused(true),
                    command => self.commands.push(command),
                }
            }
            self.ui.update_at(self.clock);
        }
    }

    pub fn take_window_commands(&mut self) -> Vec<WindowCommand> {
        let mut commands = std::mem::take(&mut self.commands);
        commands.extend(self.ui.take_window_commands());
        commands
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

    pub fn center_of(&mut self, name: &str) -> Result<Point, TestError> {
        self.advance(0.0);
        self.ui
            .bounds_of(name)
            .map(|bounds| bounds.center())
            .ok_or_else(|| TestError::NoTestId(name.to_owned()))
    }

    pub fn click_on(&mut self, name: &str) -> Result<(), TestError> {
        let point = self.center_of(name)?;
        self.click_at(point.x, point.y);
        Ok(())
    }

    pub fn click_nth(&mut self, name: &str, index: usize) -> Result<(), TestError> {
        self.advance(0.0);
        let point = self.ui.bounds_of_all(name).get(index).map(Rect::center);
        let point = point.ok_or_else(|| TestError::NoTestId(format!("{name} #{index}")))?;
        self.click_at(point.x, point.y);
        Ok(())
    }

    pub fn right_click_on(&mut self, name: &str) -> Result<(), TestError> {
        let point = self.center_of(name)?;
        self.advance(1.0);
        self.ui.pointer_moved(point);
        self.advance(0.0);
        self.ui.pointer_down(PointerButton::Secondary);
        self.ui.pointer_up(PointerButton::Secondary);
        self.advance(0.0);
        Ok(())
    }

    pub fn hover_on(&mut self, name: &str) -> Result<(), TestError> {
        let point = self.center_of(name)?;
        self.ui.pointer_moved(point);
        self.advance(0.0);
        self.ui.pointer_moved(point);
        self.advance(0.0);
        Ok(())
    }

    pub fn drag_on(&mut self, name: &str, to: Point) -> Result<(), TestError> {
        let from = self.center_of(name)?;
        self.ui.pointer_moved(from);
        self.ui.pointer_down(PointerButton::Primary);
        for step in 1..=DRAG_STEPS {
            let progress = f32::from(step) / f32::from(DRAG_STEPS);
            self.ui.pointer_moved(Point::new(
                from.x + (to.x - from.x) * progress,
                from.y + (to.y - from.y) * progress,
            ));
            self.advance(0.0);
        }
        self.ui.pointer_up(PointerButton::Primary);
        self.advance(0.0);
        Ok(())
    }

    pub fn scroll_on(&mut self, name: &str, delta: Point) -> Result<(), TestError> {
        let point = self.center_of(name)?;
        self.ui.pointer_moved(point);
        self.ui.scroll(delta);
        self.advance(0.0);
        Ok(())
    }

    pub fn scroll_into_view(&mut self, scroller: &str, name: &str) -> Result<(), TestError> {
        for _ in 0..MAX_SCROLLS {
            self.advance(0.0);
            let view = self.bounds_of(scroller);
            let view = view.ok_or_else(|| TestError::NoTestId(scroller.to_owned()))?;
            let target = self.bounds_of(name);
            let target = target.ok_or_else(|| TestError::NoTestId(name.to_owned()))?;
            let delta = if target.max().y > view.max().y {
                view.max().y - target.max().y
            } else if target.origin.y < view.origin.y {
                view.origin.y - target.origin.y
            } else {
                return Ok(());
            };
            self.scroll_on(scroller, Point::new(0.0, delta))?;
        }
        Err(TestError::NotScrolledIntoView(name.to_owned()))
    }

    pub fn drop_files_on(&mut self, name: &str, paths: Vec<PathBuf>) -> Result<(), TestError> {
        let point = self.center_of(name)?;
        self.ui.pointer_moved(point);
        self.ui.file_drop(FileDropPhase::Hover, paths.clone());
        self.ui.file_drop(FileDropPhase::Drop, paths);
        self.advance(0.0);
        Ok(())
    }

    pub fn click(&mut self, position: Point) {
        self.ui.pointer_moved(position);
        self.ui.pointer_down(PointerButton::Primary);
        self.ui.pointer_up(PointerButton::Primary);
    }

    pub fn click_at(&mut self, x: f32, y: f32) {
        self.advance(1.0);
        self.ui.pointer_moved(Point::new(x, y));
        self.advance(0.0);
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

    #[track_caller]
    pub fn press_keys(&mut self, chords: &str) {
        for chord in chords.split_whitespace() {
            self.ui.press_chord(chord);
            self.advance(0.0);
        }
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
