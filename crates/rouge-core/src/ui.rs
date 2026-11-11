use crate::clipboard::{Clipboard, ClipboardTarget};
use crate::event::{
    Chord, Cursor, DragEvent, DragPhase, ImeEvent, Key, KeyEvent, Modifiers, PointerButton,
    PointerEvent, ScrollEvent,
};
use crate::keymap::Keymap;
use crate::motion::UiContext;
use crate::style::Dirty;
use crate::theme::{Palette, provide_theme};
use crate::tree::edit::EditOp;
use crate::tree::{ActionHandler, ElementId, Event, FrameStats, On, PaintCache, Tree, emit};
use crate::view::{BuildCx, Patch, Patches, View, send};
use crate::window::{Appearance, Sent, WindowCommand, WindowHandle};
use rouge_reactive::{
    ReadSignal, Scope, Signal, batch, effect, expect_context, poll_tasks, provide_context, signal,
};
use rouge_render::{Atlas, Compositor, Item, LayerCommand, LayerId, Point, Rect, Size};
use rouge_text::TextSystem;
use std::any::{Any, TypeId};
use std::cell::Cell;
use std::collections::VecDeque;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[derive(Clone, Copy)]
struct WindowSize(ReadSignal<Size>);

pub fn window_size() -> ReadSignal<Size> {
    expect_context::<WindowSize>().0
}

#[derive(Clone, Copy)]
struct RootFontSize(Signal<f32>);

pub fn root_font_size() -> Signal<f32> {
    expect_context::<RootFontSize>().0
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
    window_commands: UnboundedReceiver<Sent>,
    window_handle: WindowHandle,
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
        Self::with_fonts(size, [], root)
    }

    pub fn with_fonts<T: View>(
        size: Size,
        fonts: impl IntoIterator<Item = Vec<u8>>,
        root: impl FnOnce() -> T,
    ) -> Ui {
        let (patches, pending) = unbounded_channel();
        let mut tree = Tree::new(size);
        let mut text_system = TextSystem::new();
        for font in fonts {
            text_system.add_font(font);
        }
        let scope = Scope::root();
        let (commands, window_commands) = unbounded_channel();
        let (size, window_handle) = scope.run(|| {
            let window_handle = WindowHandle {
                commands: signal(commands),
                title: signal(String::new()),
                appearance: signal(Appearance::Light),
            };
            provide_context(window_handle);
            let size = signal(size);
            provide_context(WindowSize(size.read_only()));
            let root_size = signal(16.0);
            provide_context(RootFontSize(root_size));
            let root_patches = patches.clone();
            effect(move || {
                let size = root_size.get();
                send(&root_patches, move |cx| cx.tree.set_root_font_size(size));
            });
            provide_context(UiContext::new(patches.clone()));
            let keymap = Keymap(signal(Vec::new()));
            provide_context(keymap);
            tree.observe_keymap(keymap);
            provide_theme(Palette::default());
            let mut cx = BuildCx {
                tree: &mut tree,
                patches: &patches,
                text: &mut text_system,
            };
            let element = root().build(&mut cx);
            tree.append_child(tree.root(), element);
            (size, window_handle)
        });
        Ui {
            window_commands,
            window_handle,
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
        self.scope.run(|| {
            batch(|| {
                while let Ok(patch) = self.pending.try_recv() {
                    let text = &mut self.text_system;
                    patch(&mut BuildCx {
                        tree: &mut self.tree,
                        patches: &self.patches,
                        text,
                    });
                }
                self.tree.restore_replaced_focus();
            });
        });
        self.tree.publish_bindings();
        if let Some(commit) = self.tree.take_commit() {
            self.compositor.apply(commit);
        }
        &mut self.tree
    }

    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.window_handle.appearance.set_if_changed(appearance);
    }

    pub fn take_window_commands(&mut self) -> Vec<WindowCommand> {
        poll_tasks();
        let mut commands = Vec::new();
        while let Ok(sent) = self.window_commands.try_recv() {
            match sent {
                Sent::Window(command) => commands.push(command),
                Sent::Copy(text) => self.tree.copy_text(&text),
            }
        }
        commands
    }

    pub fn add_font(&mut self, data: Vec<u8>) -> Vec<String> {
        self.text_system.add_font(data)
    }

    pub fn needs_frame(&mut self) -> bool {
        self.tree().dirty() != Dirty::None
            || self.compositor.is_animating()
            || self.tree.caret_animating(self.now())
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
        self.tree.tick_caret(now);
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
                self.tree
                    .set_pointer(&self.compositor, self.pointer, self.modifiers);
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

    pub fn bounds_of(&mut self, name: &str) -> Option<Rect> {
        self.bounds_of_all(name).into_iter().next()
    }

    pub fn bounds_of_all(&mut self, name: &str) -> Vec<Rect> {
        self.tree();
        self.tree.bounds_of_all(&self.compositor, name)
    }

    pub fn text_index_at(&mut self, position: Point) -> Option<usize> {
        self.tree();
        self.tree
            .hit_test(&self.compositor, position)
            .iter()
            .rev()
            .find_map(|id| self.tree.text_hit(&self.compositor, *id, position))
            .map(|hit| hit.index)
    }

    pub fn bounds_at(&mut self, position: Point) -> Option<Rect> {
        self.tree();
        let Ui {
            tree, compositor, ..
        } = self;
        tree.hit_test(compositor, position)
            .last()
            .map(|id| tree.window_bounds(compositor, *id))
    }

    fn refresh_pointer(&mut self) {
        self.tree();
        self.tree
            .set_pointer(&self.compositor, self.pointer, self.modifiers);
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

    pub fn clipboard_target(&mut self) -> Option<ClipboardTarget> {
        self.tree().clipboard_target()
    }

    pub fn clipboard_selection(&mut self) -> Option<(ClipboardTarget, String)> {
        self.tree();
        self.tree.clipboard_selection(&mut self.text_system)
    }

    pub fn paste_text(&mut self, target: ClipboardTarget, text: String) -> bool {
        self.clipboard_edit(&target, EditOp::PasteText(text))
    }

    pub fn cut_selection(&mut self, target: ClipboardTarget) -> bool {
        self.clipboard_edit(&target, EditOp::CutSelection)
    }

    fn clipboard_edit(&mut self, target: &ClipboardTarget, op: EditOp) -> bool {
        self.tree();
        let applied = self
            .scope
            .run(|| self.tree.clipboard_edit(target, op, &mut self.text_system));
        self.tree();
        applied
    }

    pub fn clear_focus(&mut self) {
        if self.tree().is_composing() {
            self.ime(ImeEvent::Preedit {
                text: String::new(),
                cursor: None,
            });
        }
        self.tree().set_focus(None, false);
    }

    pub fn text_cursor_area(&mut self) -> Option<Rect> {
        self.tree();
        self.tree.caret_area(&self.compositor)
    }

    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        if self.modifiers != modifiers {
            self.modifiers = modifiers;
            self.refresh_pointer();
        }
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

    pub fn binding_for<T: Any>(&mut self) -> Option<Chord> {
        self.tree()
            .effective_bindings()
            .into_iter()
            .find(|(kind, _)| *kind == TypeId::of::<T>())
            .map(|(_, chord)| chord)
    }

    pub fn dispatch_action<T: Any>(&mut self, action: T) -> bool {
        self.dispatch_action_ref(&action)
    }

    pub(crate) fn dispatch_action_ref(&mut self, action: &dyn Any) -> bool {
        let path = self.tree().focus_path();
        let handlers = self.tree.action_handlers(&path, action.type_id());
        self.invoke_action(action, &handlers)
    }

    fn invoke_action(&mut self, action: &dyn Any, handlers: &[ActionHandler]) -> bool {
        self.invoke(|| handlers.iter().for_each(|handler| handler(action)));
        !handlers.is_empty()
    }

    pub(crate) fn invoke(&mut self, f: impl FnOnce()) {
        self.scope.run(|| batch(f));
        self.tree();
    }

    pub fn key_down(&mut self, event: KeyEvent) -> bool {
        if event.key == Key::Enter && self.tree().is_composing() {
            return true;
        }
        if self.key_event(&event, On::KeyDown) {
            return true;
        }
        let path = self.tree().focus_path();
        if let Some((action, handlers)) = self.tree.binding(&path, &event) {
            return self.invoke_action(&*action, &handlers);
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
        self.set_modifiers(event.modifiers);
        self.emit_focused(on, Event::Key(event));
        event.stopped.get()
    }

    pub fn ime(&mut self, event: ImeEvent) {
        self.emit_focused(On::Ime, Event::Ime(&event));
    }

    fn emit_focused(&mut self, on: On, event: Event<'_>) {
        let path = self.tree().focus_path();
        self.scope
            .run(|| emit(&self.tree.listeners(&path, on), event, |_| {}));
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

    pub fn pointer_cancel(&mut self) {
        self.held = None;
        if let (Some(drag), Some(position)) =
            (self.drag.take().filter(|drag| drag.started), self.pointer)
        {
            self.drag_event(&drag, DragPhase::End, position);
        }
        self.tree.release_capture();
        self.tree.release();
        self.pointer_left();
    }

    pub fn scroll(&mut self, delta: Point) -> bool {
        if self.pointer.is_none() {
            return false;
        }
        let by = Point::ZERO - delta;
        let handled = if !self.scroll_event(delta) {
            true
        } else if let Some(layer) = self.scroll_target(by, false) {
            self.tree.scroll_command(layer, LayerCommand::ScrollBy(by));
            true
        } else {
            false
        };
        self.refresh_pointer();
        handled
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
        self.tree();
        if on == On::Click {
            self.tree
                .text_click(&self.compositor, path, position, self.modifiers);
        }
        let listeners = self.tree().listeners(path, on);
        let mut event = PointerEvent::new(position, button);
        event.modifiers = self.modifiers;
        event.click_count = self.clicks.map_or(1, |(_, _, count)| count);
        let (tree, compositor) = (&self.tree, &self.compositor);
        let target = Cell::new(None);
        self.scope.run(|| {
            emit(&listeners, Event::Pointer(&event), |element| {
                if !event.captured.get() {
                    target.set(Some(element));
                }
                let local = tree.to_local(compositor, element, position);
                event.local.set(local);
            });
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
