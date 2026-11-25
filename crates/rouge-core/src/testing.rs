use crate::access::{Action, ActionData, ActionRequest, NodeId, Role, Toggled, TreeId};
use crate::animation::{Ease, Tween};
use crate::compositor::{CompositorActor, Message, WindowConfig, WindowId};
use crate::event::{FileDropPhase, KeyEvent, PointerButton, TouchPhase};
use crate::presentation::InputKind;
use crate::theme::{Palette, Theme};
use crate::ui::Ui;
use crate::view::View;
use crate::window::{Menu, WindowCommand};
use accesskit_consumer::{NodeRef, Tree as AccessTree, TreeChangeHandler, common_filter};
use rouge_render::headless::{Image, headless_gpu};
use rouge_render::{
    Color, Command, GpuError, Point, Rect, RenderMemory, RenderStats, Size, Surfaces, Target,
};
use rouge_theme::UiPalette;
use std::borrow::Cow;
use std::iter::from_fn;
use std::mem::take;
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
    #[error("no text containing {0:?}")]
    NoText(String),
    #[error("no accessible {role:?} named {name:?}")]
    NoAccessNode { role: Role, name: String },
    #[error("{0:?} never scrolled into view")]
    NotScrolledIntoView(String),
    #[error("the test window is closed")]
    Closed,
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
const WINDOW: WindowId = WindowId(0);
const CLICK_PAUSE: f64 = 1.0;
const DRAG_STEPS: u8 = 8;
const MAX_SCROLLS: usize = 100;

pub struct TestUiBuilder {
    menus: Vec<Menu>,
    palette: Option<Palette>,
    fonts: Vec<Cow<'static, [u8]>>,
    size: Size,
    scale: f32,
    clear: Color,
    interval: f64,
    input: InputKind,
}

impl Default for TestUiBuilder {
    fn default() -> Self {
        TestUiBuilder {
            menus: Vec::new(),
            palette: None,
            fonts: Vec::new(),
            size: Size::new(200.0, 100.0),
            scale: 1.0,
            clear: Color::TRANSPARENT,
            interval: STEP,
            input: InputKind::default(),
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

    pub fn refresh_rate(mut self, hertz: f64) -> Self {
        self.interval = 1.0 / hertz;
        self
    }

    pub fn fonts<T: Into<Cow<'static, [u8]>>>(
        mut self,
        fonts: impl IntoIterator<Item = T>,
    ) -> Self {
        self.fonts.extend(fonts.into_iter().map(Into::into));
        self
    }

    pub fn input(mut self, input: InputKind) -> Self {
        self.input = input;
        self
    }

    pub fn menus(mut self, menus: Vec<Menu>) -> Self {
        self.menus = menus;
        self
    }

    pub fn palette<T: Into<Color>>(mut self, palette: UiPalette<T>) -> Self {
        self.palette = Some(palette.map(Into::into));
        self
    }

    pub fn build<T: View>(self, root: impl FnOnce() -> T) -> TestUi {
        self.build_with(|| (root(), ())).0
    }

    pub fn build_with<T: View, U>(self, root: impl FnOnce() -> (T, U)) -> (TestUi, U) {
        let mut extra = None;
        let (palette, input) = (self.palette, self.input);
        let mut ui = Ui::with_input(self.size, input, self.fonts, || {
            if let Some(palette) = palette {
                Theme::current().set(palette);
            }
            let (view, value) = root();
            extra = Some(value);
            view
        });
        ui.pin_clock(0.0);
        ui.set_device_scale(self.scale);
        ui.bind_menus(&self.menus);
        let mut ui = TestUi {
            ui,
            size: self.size,
            scale: self.scale,
            clear: self.clear,
            interval: self.interval,
            actor: None,
            clock: 0.0,
            commands: Vec::new(),
            access: None,
        };
        ui.advance(0.0);
        (ui, extra.expect("the root view was built"))
    }
}

pub struct TestUi {
    ui: Ui,
    size: Size,
    scale: f32,
    clear: Color,
    interval: f64,
    actor: Option<CompositorActor>,
    clock: f64,
    commands: Vec<WindowCommand>,
    access: Option<AccessTree>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccessNode {
    pub role: Role,
    pub name: String,
    pub value: Option<String>,
    pub description: Option<String>,
    pub bounds: Rect,
    pub focused: bool,
    pub disabled: bool,
    pub toggled: Option<Toggled>,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub numeric_value: Option<f64>,
    pub actions: Vec<Action>,
    id: NodeId,
}

impl TestUi {
    pub fn new<T: View>(root: impl FnOnce() -> T) -> TestUi {
        TestUiBuilder::default().build(root)
    }

    pub fn canvas<T: View>(root: impl FnOnce() -> T) -> TestUi {
        TestUiBuilder::default().clear(Color::WHITE).build(root)
    }

    fn actor(&mut self) -> Result<&mut CompositorActor, TestError> {
        let config = WindowConfig {
            width: (self.size.width * self.scale) as u32,
            height: (self.size.height * self.scale) as u32,
            interval: Duration::from_secs_f64(self.interval),
            visible: true,
        };
        if self.actor.is_none() {
            let gpu = headless_gpu()?;
            let target = Target::offscreen(&gpu, config.width, config.height);
            let mut actor = CompositorActor::new(gpu.clone(), Surfaces::new(&gpu));
            actor.handle(Message::Open(WINDOW, target, config));
            self.actor = Some(actor);
        }
        let actor = self.actor.as_mut().ok_or(TestError::Closed)?;
        actor.handle(Message::Configure(WINDOW, config));
        Ok(actor)
    }

    /// Resizes the window, as the platform does: the layout and the render target both follow.
    pub fn set_size(&mut self, size: Size) {
        self.size = size;
        self.ui.set_size(size);
    }

    pub fn surfaces(&mut self) -> Result<&mut Surfaces, TestError> {
        Ok(self.actor()?.surfaces())
    }

    fn sync(&mut self) -> Result<(), TestError> {
        let mut commit = self.ui.commit();
        commit.clear = Some(self.clear);
        self.actor()?.handle(Message::Commit(WINDOW, commit));
        Ok(())
    }

    fn present(&mut self, at: f64) -> Result<&mut CompositorActor, TestError> {
        self.actor()?.draw(WINDOW, at);
        let (actor, ui) = (self.actor.as_mut().ok_or(TestError::Closed)?, &mut self.ui);
        actor.events().for_each(|(_, event)| ui.handle(event));
        Ok(actor)
    }

    pub fn draw(&mut self) -> Result<RenderStats, TestError> {
        self.sync()?;
        Ok(self.present(self.clock)?.stats())
    }

    pub fn composite(&mut self, seconds: f64) -> Result<Image, TestError> {
        let actor = self.present(self.clock + seconds)?;
        Ok(actor.image(WINDOW).ok_or(TestError::Closed)??)
    }

    pub fn compositor_frames(&mut self, seconds: f64) -> Result<usize, TestError> {
        let end = self.clock + seconds;
        let mut now = self.clock;
        let mut frames = 0;
        while let Some(at) = self.actor()?.next_wake(now)
            && at < end
        {
            now = at.max(now);
            self.actor()?.frame(now);
            let (actor, ui) = (self.actor.as_mut().ok_or(TestError::Closed)?, &mut self.ui);
            actor.events().for_each(|(_, event)| ui.handle(event));
            frames += 1;
        }
        Ok(frames)
    }

    pub fn memory(&mut self) -> Result<RenderMemory, TestError> {
        Ok(self.actor()?.memory(WINDOW).unwrap_or_default())
    }

    pub fn wait(&self) -> Result<(), TestError> {
        if let Some(actor) = &self.actor {
            actor.gpu().wait()?;
        }
        Ok(())
    }

    pub fn commands(&mut self) -> Result<Vec<Command>, TestError> {
        self.sync()?;
        let clock = self.clock;
        Ok(self.actor()?.commands(WINDOW, clock).unwrap_or_default())
    }

    pub fn render(&mut self) -> Result<Image, TestError> {
        self.sync()?;
        self.composite(0.0)
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
        self.ui.flush();
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
        let mut commands = take(&mut self.commands);
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

    pub fn rect_of(&mut self, name: &str) -> Result<Rect, TestError> {
        self.advance(0.0);
        self.ui
            .bounds_of(name)
            .ok_or_else(|| TestError::NoTestId(name.to_owned()))
    }

    pub fn visible_rect_of(&mut self, name: &str) -> Result<Rect, TestError> {
        self.advance(0.0);
        self.ui
            .visible_bounds_of(name)
            .ok_or_else(|| TestError::NoTestId(name.to_owned()))
    }

    pub fn center_of(&mut self, name: &str) -> Result<Point, TestError> {
        Ok(self.rect_of(name)?.center())
    }

    pub fn click_on(&mut self, name: &str) -> Result<(), TestError> {
        self.click_nth(name, 0)
    }

    pub fn click_nth(&mut self, name: &str, index: usize) -> Result<(), TestError> {
        self.advance(0.0);
        let point = self.ui.bounds_of_all(name).get(index).map(Rect::center);
        let point = point.ok_or_else(|| TestError::NoTestId(format!("{name} #{index}")))?;
        self.press_pointer(point, PointerButton::Primary);
        Ok(())
    }

    pub fn click_text(&mut self, needle: &str) -> Result<(), TestError> {
        self.advance(0.0);
        let point = self.ui.text_bounds(needle).map(|bounds| bounds.center());
        let point = point.ok_or_else(|| TestError::NoText(needle.to_owned()))?;
        self.press_pointer(point, PointerButton::Primary);
        Ok(())
    }

    pub fn double_click_on(&mut self, name: &str) -> Result<(), TestError> {
        let point = self.center_of(name)?;
        self.press_pointer(point, PointerButton::Primary);
        self.ui.pointer_down(PointerButton::Primary);
        self.ui.pointer_up(PointerButton::Primary);
        self.advance(0.0);
        Ok(())
    }

    pub fn right_click_on(&mut self, name: &str) -> Result<(), TestError> {
        let point = self.center_of(name)?;
        self.press_pointer(point, PointerButton::Secondary);
        Ok(())
    }

    pub fn click_at(&mut self, x: f32, y: f32) {
        self.press_pointer(Point::new(x, y), PointerButton::Primary);
    }

    pub fn click(&mut self, position: Point) {
        self.ui.pointer_moved(position);
        self.ui.pointer_down(PointerButton::Primary);
        self.ui.pointer_up(PointerButton::Primary);
    }

    pub fn hover_on(&mut self, name: &str) -> Result<(), TestError> {
        let point = self.center_of(name)?;
        self.settle_pointer(point);
        Ok(())
    }

    pub fn click_times(&mut self, position: Point, count: u32) {
        self.advance(CLICK_PAUSE);
        self.settle_pointer(position);
        for _ in 0..count {
            self.ui.pointer_down(PointerButton::Primary);
            self.ui.pointer_up(PointerButton::Primary);
        }
        self.advance(0.0);
    }

    pub fn drag(&mut self, from: Point, to: Point) {
        self.advance(CLICK_PAUSE);
        self.drag_between(from, to);
    }

    pub fn drag_on(&mut self, name: &str, to: Point) -> Result<(), TestError> {
        let from = self.center_of(name)?;
        self.drag_between(from, to);
        Ok(())
    }

    fn drag_between(&mut self, from: Point, to: Point) {
        self.ui.pointer_moved(from);
        self.ui.pointer_down(PointerButton::Primary);
        for point in steps(from, to) {
            self.ui.pointer_moved(point);
            self.advance(0.0);
        }
        self.ui.pointer_up(PointerButton::Primary);
        self.advance(0.0);
    }

    pub fn swipe(&mut self, from: Point, to: Point) {
        self.swipe_over(from, to, 0.0);
    }

    pub fn swipe_over(&mut self, from: Point, to: Point, seconds: f64) {
        self.ui.touch(TouchPhase::Start, from);
        self.advance(0.0);
        let pause = seconds / f64::from(DRAG_STEPS);
        for point in steps(from, to) {
            self.advance(pause);
            self.ui.touch(TouchPhase::Move, point);
        }
        self.ui.touch(TouchPhase::End, to);
        self.advance(0.0);
    }

    pub fn tap_on(&mut self, name: &str) -> Result<(), TestError> {
        let point = self.center_of(name)?;
        self.tap(point);
        Ok(())
    }

    pub fn tap(&mut self, point: Point) {
        self.ui.touch(TouchPhase::Start, point);
        self.ui.touch(TouchPhase::End, point);
        self.advance(0.0);
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

    #[track_caller]
    pub fn press(&mut self, chords: &str) -> bool {
        let mut handled = true;
        for chord in chords.split_whitespace() {
            handled &= self.ui.press_chord(chord);
            self.advance(0.0);
        }
        handled
    }

    pub fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.ui.key_down(KeyEvent::typed(&character.to_string()));
        }
        self.advance(0.0);
    }

    pub fn access_nodes(&mut self) -> Vec<AccessNode> {
        self.access_snapshots().collect()
    }

    fn access_snapshots(&mut self) -> impl Iterator<Item = AccessNode> + '_ {
        self.advance(0.0);
        let scale = self.ui.scale();
        let root = self.access_tree().state().root();
        let mut pending: Vec<_> = root.filtered_children(common_filter).rev().collect();
        from_fn(move || {
            let node = pending.pop()?;
            pending.extend(node.filtered_children(common_filter).rev());
            Some(access_node(&node, scale))
        })
    }

    pub fn access_node(&mut self, role: Role, name: &str) -> Result<AccessNode, TestError> {
        self.access_snapshots()
            .find(|node| node.role == role && node.name == name)
            .ok_or_else(|| TestError::NoAccessNode {
                role,
                name: name.to_owned(),
            })
    }

    pub fn perform(&mut self, role: Role, name: &str, action: Action) -> Result<bool, TestError> {
        self.request(role, name, action, None)
    }

    pub fn perform_with(
        &mut self,
        role: Role,
        name: &str,
        action: Action,
        data: ActionData,
    ) -> Result<bool, TestError> {
        self.request(role, name, action, Some(data))
    }

    fn request(
        &mut self,
        role: Role,
        name: &str,
        action: Action,
        data: Option<ActionData>,
    ) -> Result<bool, TestError> {
        let node = self.access_node(role, name)?;
        let handled = self.ui.accessibility_action(ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: node.id,
            data,
        });
        self.advance(0.0);
        Ok(handled)
    }

    pub fn stop_accessibility(&mut self) {
        self.access = None;
        self.ui.stop_accessibility();
    }

    pub fn access_tree(&mut self) -> &AccessTree {
        let ui = &mut self.ui;
        match &mut self.access {
            Some(tree) => {
                if let Some(update) = ui.accessibility_update() {
                    tree.update_and_process_changes(update, &mut Unobserved);
                }
                tree
            }
            access => access.insert(AccessTree::new(ui.accessibility_tree(), true)),
        }
    }

    fn press_pointer(&mut self, point: Point, button: PointerButton) {
        self.advance(CLICK_PAUSE);
        self.settle_pointer(point);
        self.ui.pointer_down(button);
        self.ui.pointer_up(button);
        self.advance(0.0);
    }

    fn settle_pointer(&mut self, point: Point) {
        self.ui.pointer_moved(point);
        self.advance(0.0);
        self.ui.pointer_moved(point);
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

fn steps(from: Point, to: Point) -> impl Iterator<Item = Point> {
    (1..=DRAG_STEPS).map(move |step| {
        let progress = f32::from(step) / f32::from(DRAG_STEPS);
        Point::new(
            from.x + (to.x - from.x) * progress,
            from.y + (to.y - from.y) * progress,
        )
    })
}

const ACTIONS: [Action; 8] = [
    Action::Click,
    Action::Focus,
    Action::Increment,
    Action::Decrement,
    Action::SetValue,
    Action::SetTextSelection,
    Action::ReplaceSelectedText,
    Action::ScrollIntoView,
];

struct Unobserved;

impl TreeChangeHandler for Unobserved {
    fn node_added(&mut self, _node: &NodeRef<'_>) {}

    fn node_updated(&mut self, _old: &NodeRef<'_>, _new: &NodeRef<'_>) {}

    fn focus_moved(&mut self, _old: Option<&NodeRef<'_>>, _new: Option<&NodeRef<'_>>) {}

    fn node_removed(&mut self, _node: &NodeRef<'_>) {}
}

fn access_node(node: &NodeRef<'_>, scale: f32) -> AccessNode {
    let (id, _) = node.locate();
    let bounds = node.bounding_box().map_or(Rect::ZERO, |bounds| {
        Rect::new(
            bounds.x0 as f32 / scale,
            bounds.y0 as f32 / scale,
            (bounds.x1 - bounds.x0) as f32 / scale,
            (bounds.y1 - bounds.y0) as f32 / scale,
        )
    });
    let data = node.data();
    AccessNode {
        role: node.role(),
        name: node.label().unwrap_or_default(),
        value: node.value(),
        description: node.description(),
        bounds,
        focused: node.is_focused(),
        disabled: node.is_disabled(),
        toggled: node.toggled(),
        selected: node.is_selected(),
        expanded: data.is_expanded(),
        numeric_value: node.numeric_value(),
        actions: ACTIONS
            .into_iter()
            .filter(|action| data.supports_action(*action))
            .collect(),
        id,
    }
}
