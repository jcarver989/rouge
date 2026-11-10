use crate::clipboard::Clipboard;
use crate::event::{
    Cursor, DragEvent, DragPhase, ImeEvent, Key, KeyEvent, Modifiers, PointerButton, PointerEvent,
    ScrollEvent,
};
use crate::motion::UiContext;
use crate::style::Dirty;
use crate::theme::{Palette, provide_theme};
use crate::tree::{ElementId, Event, FrameStats, On, PaintCache, Tree, emit};
use crate::view::{BuildCx, Patch, Patches, View};
use rouge_reactive::{
    ReadSignal, Scope, Signal, batch, expect_context, poll_tasks, provide_context, signal,
};
use rouge_render::{Atlas, Compositor, Item, LayerCommand, LayerId, Point, Rect, Size};
use rouge_text::TextSystem;
use std::cell::Cell;
use std::collections::VecDeque;
use std::time::Instant;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

#[derive(Clone, Copy)]
struct WindowSize(ReadSignal<Size>);

pub fn window_size() -> ReadSignal<Size> {
    expect_context::<WindowSize>().0
}

const MAX_PASSES: usize = 8;
const DRAG_SLOP: f32 = 3.0;
const VELOCITY_WINDOW: f64 = 0.1;
const MULTI_CLICK_SECONDS: f64 = 0.4;
const MULTI_CLICK_SLOP: f32 = 4.0;

struct Drag {
    element: ElementId,
    start: Point,
    last: Point,
    started: bool,
    samples: VecDeque<(f64, Point)>,
}

impl Drag {
    fn velocity(&self) -> Point {
        let (Some((from, first)), Some((to, last))) = (self.samples.front(), self.samples.back())
        else {
            return Point::ZERO;
        };
        let elapsed = (to - from) as f32;
        if elapsed <= 1.0e-4 {
            return Point::ZERO;
        }
        Point::new((last.x - first.x) / elapsed, (last.y - first.y) / elapsed)
    }
}

pub struct Ui {
    tree: Tree,
    compositor: Compositor,
    size: Signal<Size>,
    patches: Patches,
    pending: UnboundedReceiver<Patch>,
    scope: Scope,
    cache: PaintCache,
    text_system: TextSystem,
    pointer: Option<Point>,
    held: Option<PointerButton>,
    outside: bool,
    drag: Option<Drag>,
    scrolling: Option<LayerId>,
    modifiers: Modifiers,
    clicks: Option<(f64, Point, u32)>,
    started: Instant,
    clock: Option<f64>,
    last_tick: Option<f64>,
}

pub struct Frame<'a> {
    pub scene: Vec<Item<'a>>,
    pub atlas: &'a mut Atlas,
}

impl Ui {
    pub fn new<T: View>(size: Size, root: impl FnOnce() -> T) -> Ui {
        let (patches, pending) = unbounded_channel();
        let mut tree = Tree::new(size);
        let mut text_system = TextSystem::new();
        let scope = Scope::root();
        let size = scope.run(|| {
            let size = signal(size);
            provide_context(WindowSize(size.read_only()));
            provide_context(UiContext::new(patches.clone()));
            provide_theme(Palette::default());
            let mut cx = BuildCx {
                tree: &mut tree,
                patches: &patches,
                text: &mut text_system,
            };
            let element = root().build(&mut cx);
            tree.append_child(tree.root(), element);
            size
        });
        Ui {
            tree,
            compositor: Compositor::new(),
            size,
            patches,
            pending,
            scope,
            cache: PaintCache::new(1.0),
            text_system,
            pointer: None,
            held: None,
            outside: false,
            drag: None,
            scrolling: None,
            modifiers: Modifiers::NONE,
            clicks: None,
            started: Instant::now(),
            clock: None,
            last_tick: None,
        }
    }

    fn tree(&mut self) -> &mut Tree {
        poll_tasks();
        while let Ok(patch) = self.pending.try_recv() {
            let text = &mut self.text_system;
            patch(&mut BuildCx {
                tree: &mut self.tree,
                patches: &self.patches,
                text,
            });
        }
        if let Some(commit) = self.tree.take_commit() {
            self.compositor.apply(commit);
        }
        &mut self.tree
    }

    pub fn add_font(&mut self, data: Vec<u8>) -> Vec<String> {
        self.text_system.add_font(data)
    }

    pub fn needs_frame(&mut self) -> bool {
        self.tree().dirty() != Dirty::None || self.compositor.is_animating()
    }

    pub fn is_animating(&mut self) -> bool {
        self.tree();
        self.compositor.is_animating()
    }

    pub fn set_size(&mut self, size: Size) {
        self.size.set_if_changed(size);
        self.tree().set_size(size);
    }

    pub fn set_scale(&mut self, scale: f32) {
        if self.cache.scale != scale {
            self.cache.scale = scale;
            self.tree().invalidate_paint();
        }
    }

    pub fn scale(&self) -> f32 {
        self.cache.scale
    }

    pub fn update(&mut self) -> bool {
        self.clock = None;
        self.step(self.started.elapsed().as_secs_f64())
    }

    pub fn update_at(&mut self, now: f64) -> bool {
        self.clock = Some(now);
        self.step(now)
    }

    fn now(&self) -> f64 {
        self.clock
            .unwrap_or_else(|| self.started.elapsed().as_secs_f64())
    }

    fn step(&mut self, now: f64) -> bool {
        self.tree();
        let mut changed = self.compositor.is_animating();
        if changed {
            let dt = self.last_tick.map_or(0.0, |last| (now - last).max(0.0));
            self.compositor.tick(dt as f32);
        }
        for _ in 0..MAX_PASSES {
            self.tree.publish(&self.compositor);
            let relayout = self.tree().dirty().needs_layout();
            if !self.tree.update(&mut self.cache, &mut self.text_system) {
                break;
            }
            changed = true;
            self.tree();
            if relayout && self.pointer.is_some() {
                self.tree.set_pointer(&self.compositor, self.pointer);
            }
        }
        self.last_tick = self.compositor.is_animating().then_some(now);
        changed
    }

    pub fn frame(&mut self) -> Frame<'_> {
        Frame {
            scene: self.compositor.scene(self.cache.scale),
            atlas: &mut self.cache.atlas,
        }
    }

    pub fn stats(&mut self) -> FrameStats {
        self.tree().stats()
    }

    pub fn bounds_at(&mut self, position: Point) -> Option<Rect> {
        self.tree();
        let Ui {
            tree, compositor, ..
        } = self;
        tree.hit_test(compositor, position)
            .last()
            .and_then(|id| tree.bounds(*id))
    }

    fn refresh_pointer(&mut self) {
        self.tree();
        self.tree.set_pointer(&self.compositor, self.pointer);
    }

    pub fn pointer_moved(&mut self, position: Point) {
        self.pointer = Some(position);
        self.outside = false;
        self.refresh_pointer();
        let path = self.tree.pointer_path().to_vec();
        let button = self.held.unwrap_or(PointerButton::Primary);
        self.dispatch(&path, On::PointerMove, position, button);
        self.drag_moved(position);
    }

    pub fn pointer_left(&mut self) {
        self.outside = true;
        if self.held.is_none() {
            self.pointer = None;
            self.refresh_pointer();
        }
    }

    pub fn set_clipboard(&mut self, clipboard: impl Clipboard + 'static) {
        self.tree.set_clipboard(Box::new(clipboard));
    }

    pub fn text_cursor_area(&mut self) -> Option<Rect> {
        self.tree();
        self.tree.caret_area(&self.compositor)
    }

    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
    }

    pub fn cursor(&mut self) -> Cursor {
        self.tree().cursor()
    }

    pub fn has_focus(&mut self) -> bool {
        self.tree().focused().is_some()
    }

    pub fn focus_next(&mut self, reverse: bool) -> bool {
        self.tree().focus_next(reverse)
    }

    pub fn accepts_text(&mut self) -> bool {
        let path = self.tree().focus_path();
        self.tree.focused().is_some() && !self.tree.listeners(&path, On::Ime).is_empty()
    }

    pub fn key_down(&mut self, event: KeyEvent) -> bool {
        if self.key_event(&event, On::KeyDown) {
            return true;
        }
        let path = self.tree().focus_path();
        if let Some((action, handlers)) = self.tree.binding(&path, &event) {
            batch(|| handlers.iter().for_each(|handler| handler(&*action)));
            self.tree();
            return true;
        }
        let Modifiers {
            shift,
            ctrl,
            alt,
            meta,
        } = event.modifiers;
        event.key == Key::Tab && !(ctrl || alt || meta) && self.tree().focus_next(shift)
    }

    pub fn key_up(&mut self, event: KeyEvent) -> bool {
        self.key_event(&event, On::KeyUp)
    }

    fn key_event(&mut self, event: &KeyEvent, on: On) -> bool {
        self.modifiers = event.modifiers;
        self.emit_focused(on, Event::Key(event));
        event.stopped.get()
    }

    pub fn ime(&mut self, event: ImeEvent) {
        self.emit_focused(On::Ime, Event::Ime(&event));
    }

    fn emit_focused(&mut self, on: On, event: Event<'_>) {
        let path = self.tree().focus_path();
        emit(&self.tree.listeners(&path, on), event, |_| {});
        self.tree();
    }

    pub fn pointer_down(&mut self, button: PointerButton) {
        let Some(position) = self.pointer else { return };
        self.held = Some(button);
        let now = self.now();
        let tree = self.tree();
        if button == PointerButton::Primary {
            tree.press();
            tree.focus_pointer();
            let repeat = self.clicks.filter(|(at, last, _)| {
                let travel = position - *last;
                now - at <= MULTI_CLICK_SECONDS && travel.x.hypot(travel.y) <= MULTI_CLICK_SLOP
            });
            self.clicks = Some((now, position, repeat.map_or(1, |(_, _, count)| count + 1)));
        }
        let tree = &mut self.tree;
        let path = tree.hover_path().to_vec();
        self.dispatch(&path, On::PointerDown, position, button);
        if button == PointerButton::Primary {
            let samples = VecDeque::from([(self.now(), position)]);
            let targets = self.tree().listeners(&path, On::Drag);
            self.drag = targets.first().map(|(element, _)| Drag {
                element: *element,
                start: position,
                last: position,
                started: false,
                samples,
            });
        }
    }

    pub fn pointer_up(&mut self, button: PointerButton) {
        let Some(position) = self.pointer else { return };
        self.held = None;
        let primary = button == PointerButton::Primary;
        let dragged = self.drag.take_if(|_| primary).filter(|drag| drag.started);
        if let Some(drag) = &dragged {
            self.drag_event(drag, DragPhase::End, position);
        }
        let path = self.tree().pointer_path().to_vec();
        self.dispatch(&path, On::PointerUp, position, button);
        self.tree.release_capture();
        self.refresh_pointer();
        let clicked = if primary {
            self.tree.release()
        } else {
            Vec::new()
        };
        if dragged.is_none() {
            self.dispatch(&clicked, On::Click, position, button);
        }
        if self.outside {
            self.pointer_left();
        }
    }

    pub fn scroll(&mut self, delta: Point) {
        let by = Point::ZERO - delta;
        if self.scroll_event(delta)
            && let Some(layer) = self.scroll_target(by, false)
        {
            self.tree.scroll_command(layer, LayerCommand::ScrollBy(by));
        }
        self.refresh_pointer();
    }

    pub fn scroll_drag(&mut self, delta: Point) {
        let by = Point::ZERO - delta;
        if self.scroll_event(delta) {
            self.scrolling = self.scrolling.or_else(|| self.scroll_target(by, true));
            if let Some(layer) = self.scrolling {
                self.tree
                    .scroll_command(layer, LayerCommand::ScrollDrag(by));
            }
        }
        self.refresh_pointer();
    }

    pub fn scroll_release(&mut self, velocity: Point) {
        if let Some(layer) = self.scrolling.take() {
            self.tree()
                .scroll_command(layer, LayerCommand::ScrollRelease(Point::ZERO - velocity));
        }
    }

    fn scroll_event(&mut self, delta: Point) -> bool {
        let Some(position) = self.pointer else {
            return false;
        };
        let tree = self.tree();
        let listeners = tree.listeners(tree.hover_path(), On::Scroll);
        let event = ScrollEvent::new(position, delta);
        emit(&listeners, Event::Scroll(&event), |_| {});
        !event.stopped.get()
    }

    fn scroll_target(&mut self, by: Point, elastic: bool) -> Option<LayerId> {
        let targets = self.tree().scroll_targets();
        let moves = |layer: &&LayerId| {
            self.compositor
                .scroll(**layer)
                .is_some_and(|s| s.clamp(s.offset + by) != s.offset)
        };
        targets
            .iter()
            .find(moves)
            .or(targets.first().filter(|_| elastic))
            .copied()
    }

    fn drag_moved(&mut self, position: Point) {
        let now = self.now();
        let Some(drag) = &mut self.drag else { return };
        drag.samples.push_back((now, position));
        while drag
            .samples
            .front()
            .is_some_and(|(at, _)| now - at > VELOCITY_WINDOW)
        {
            drag.samples.pop_front();
        }
        let travel = position - drag.start;
        if !drag.started && travel.x.hypot(travel.y) < DRAG_SLOP {
            return;
        }
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        if !drag.started {
            drag.started = true;
            let path = self.tree.hover_path().to_vec();
            self.tree.capture(&path, drag.element);
            self.drag_event(&drag, DragPhase::Start, drag.start);
        }
        self.drag_event(&drag, DragPhase::Move, position);
        drag.last = position;
        self.drag = Some(drag);
    }

    fn drag_event(&mut self, drag: &Drag, phase: DragPhase, position: Point) {
        let listeners = self.tree().listeners(&[drag.element], On::Drag);
        let event = DragEvent {
            phase,
            position,
            start: drag.start,
            delta: if phase == DragPhase::Move {
                position - drag.last
            } else {
                Point::ZERO
            },
            translation: position - drag.start,
            velocity: drag.velocity(),
        };
        emit(&listeners, Event::Drag(&event), |_| {});
    }

    fn dispatch(&mut self, path: &[ElementId], on: On, position: Point, button: PointerButton) {
        let listeners = self.tree().listeners(path, on);
        let mut event = PointerEvent::new(position, button);
        event.modifiers = self.modifiers;
        event.click_count = self.clicks.map_or(1, |(_, _, count)| count);
        let (tree, compositor) = (&self.tree, &self.compositor);
        let target = Cell::new(None);
        emit(&listeners, Event::Pointer(&event), |element| {
            if !event.captured.get() {
                target.set(Some(element));
            }
            let local = tree.to_local(compositor, element, position);
            event.local.set(local);
        });
        let captured = target.take().filter(|_| event.captured.get());
        if let Some(element) = captured.filter(|_| on == On::PointerDown) {
            self.tree.capture(path, element);
        }
        self.tree();
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        self.scope.dispose();
    }
}
