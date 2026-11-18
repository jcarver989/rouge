use crate::clipboard::Clipboard;
use crate::compositor::{Commit, CompositorEvent, LayerId};
use crate::event::{
    Chord, Cursor, DragAxis, DragEvent, DragPhase, FileDropEvent, FileDropPhase, ImeEvent,
    IntoChord, Key, KeyEvent, Modifiers, PointerButton, PointerEvent, ScrollEvent, TouchPhase,
};
use crate::keymap::Keymap;
use crate::motion::UiContext;
use crate::presentation::{InputKind, Presentation, Presented, WidthClass};
use crate::style::Dirty;
use crate::text::TextSystem;
use crate::theme::Theme;
use crate::tree::{
    ActionHandler, ElementId, Event, FrameStats, Listeners, On, PaintCache, Tree, emit,
};
use crate::view::{BuildCx, Queue, View};
use crate::window::{Appearance, EditAction, Menu, MenuItem, Sent, WindowCommand, WindowHandle};
use rouge_reactive::{
    ReadSignal, Scope, Signal, batch, expect_context, poll_tasks, provide_context, signal, stats,
};
use rouge_render::{Color, Edges, Point, Rect, Size};
use rouge_theme::UiPalette;
use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::VecDeque;
use std::iter::empty;
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use tracing::trace_span;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[derive(Clone, Copy)]
struct WindowSize(ReadSignal<Size>);

pub fn window_size() -> ReadSignal<Size> {
    expect_context::<WindowSize>().0
}

#[derive(Clone, Copy)]
struct SafeArea(ReadSignal<Edges>);

pub fn safe_area() -> ReadSignal<Edges> {
    expect_context::<SafeArea>().0
}

#[derive(Clone, Copy)]
struct Zoom(Signal<f32>);

pub fn zoom() -> Signal<f32> {
    expect_context::<Zoom>().0
}

#[derive(Clone, Copy)]
struct HeldModifiers(ReadSignal<Modifiers>);

pub fn modifiers() -> ReadSignal<Modifiers> {
    expect_context::<HeldModifiers>().0
}

const DRAG_SLOP: f32 = 3.0;
const TOUCH_SLOP: f32 = 8.0;
const VELOCITY_WINDOW: f64 = 0.1;
const MULTI_CLICK_SECONDS: f64 = 0.4;
const MULTI_CLICK_SLOP: f32 = 4.0;

#[derive(Default)]
struct Samples(VecDeque<(f64, Point)>);

impl Samples {
    fn new(at: f64, position: Point) -> Self {
        Samples(VecDeque::from([(at, position)]))
    }

    fn push(&mut self, at: f64, position: Point) {
        self.0.push_back((at, position));
        while self
            .0
            .front()
            .is_some_and(|(from, _)| at - from > VELOCITY_WINDOW)
        {
            self.0.pop_front();
        }
    }

    fn velocity(&self, now: f64) -> Point {
        let (Some((from, first)), Some((to, last))) = (self.0.front(), self.0.back()) else {
            return Point::ZERO;
        };
        let elapsed = (to - from) as f32;
        if elapsed <= 1.0e-4 || now - to > VELOCITY_WINDOW {
            return Point::ZERO;
        }
        Point::new((last.x - first.x) / elapsed, (last.y - first.y) / elapsed)
    }
}

struct Drag {
    element: ElementId,
    axis: Option<DragAxis>,
    start: Point,
    last: Point,
    started: bool,
    samples: Samples,
}

struct Touch {
    start: Point,
    last: Point,
    panning: bool,
}

struct ScrollDrag {
    layer: Option<LayerId>,
    travel: Point,
    samples: Samples,
}

pub struct Ui {
    window_handle: WindowHandle,
    tree: Tree,
    size: Signal<Size>,
    viewport: Size,
    safe_area: Signal<Edges>,
    insets: Edges,
    presentation: Signal<Presentation>,
    device_scale: f32,
    zoom: Signal<f32>,
    zoomed: f32,
    queue: Queue,
    scope: Scope,
    cache: PaintCache,
    text_system: TextSystem,
    pointer: Option<Point>,
    held: Option<PointerButton>,
    outside: bool,
    drag: Option<Drag>,
    scrolling: Option<ScrollDrag>,
    touch: Option<Touch>,
    modifiers: Signal<Modifiers>,
    clicks: Option<(f64, Point, u32)>,
    epoch: Instant,
    clock: Option<f64>,
    following: bool,
    committed_clear: Option<Color>,
    committed_scale: Option<f32>,
    shortcuts: Vec<(Chord, MenuItem)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wake {
    Idle,
    At(f64),
    Now,
}

impl Wake {
    pub fn sooner(self, other: Wake) -> Wake {
        match (self, other) {
            (Wake::Now, _) | (_, Wake::Now) => Wake::Now,
            (Wake::At(a), Wake::At(b)) => Wake::At(a.min(b)),
            (Wake::At(at), Wake::Idle) | (Wake::Idle, Wake::At(at)) => Wake::At(at),
            (Wake::Idle, Wake::Idle) => Wake::Idle,
        }
    }
}

impl Ui {
    pub fn new<T: View>(size: Size, root: impl FnOnce() -> T) -> Ui {
        Self::with_input(size, InputKind::default(), empty::<&'static [u8]>(), root)
    }

    pub fn with_input<T: View, U: Into<Cow<'static, [u8]>>>(
        size: Size,
        input: InputKind,
        fonts: impl IntoIterator<Item = U>,
        root: impl FnOnce() -> T,
    ) -> Ui {
        let queue = Queue::default();
        let mut tree = Tree::new(size, queue.clone());
        let mut text_system = TextSystem::new();
        for font in fonts {
            text_system.add_font(font);
        }
        let scope = Scope::root();
        let viewport = size;
        let (size, safe_area, presentation, zoom, modifiers, window_handle) = scope.run(|| {
            let window_handle = WindowHandle {
                queue: signal(queue.clone()),
                title: signal(String::new()),
                route: signal(String::new()),
                appearance: signal(Appearance::Light),
                focused: signal(true),
            };
            provide_context(window_handle);
            let size = signal(size);
            provide_context(WindowSize(size.read_only()));
            let safe_area = signal(Edges::ZERO);
            provide_context(SafeArea(safe_area.read_only()));
            let presentation = signal(Presentation {
                input,
                width: WidthClass::of(size.get_untracked()),
            });
            provide_context(Presented(presentation.read_only()));
            let zoom = signal(1.0);
            provide_context(Zoom(zoom));
            let modifiers = signal(Modifiers::NONE);
            provide_context(HeldModifiers(modifiers.read_only()));
            provide_context(UiContext::new(queue.clone()));
            let keymap = Keymap(signal(Vec::new()));
            provide_context(keymap);
            tree.observe_keymap(keymap);
            let theme = Theme::provide(UiPalette::light());
            tree.set_theme(theme);
            let mut cx = BuildCx {
                parent: tree.content(),
                tree: &mut tree,
                queue: &queue,
                text: &mut text_system,
                theme: Some((theme, None)),
            };
            root().build(&mut cx);
            (
                size,
                safe_area,
                presentation,
                zoom,
                modifiers,
                window_handle,
            )
        });
        Ui {
            window_handle,
            tree,
            size,
            viewport,
            safe_area,
            insets: Edges::ZERO,
            presentation,
            device_scale: 1.0,
            zoom,
            zoomed: 1.0,
            queue,
            scope,
            cache: PaintCache::new(1.0),
            text_system,
            pointer: None,
            held: None,
            outside: false,
            drag: None,
            scrolling: None,
            touch: None,
            modifiers,
            clicks: None,
            epoch: Instant::now(),
            clock: None,
            following: false,
            committed_clear: None,
            committed_scale: None,
            shortcuts: Vec::new(),
        }
    }

    pub fn update(&mut self) -> bool {
        self.clock = None;
        self.step(self.epoch.elapsed().as_secs_f64())
    }

    pub fn update_at(&mut self, now: f64) -> bool {
        self.clock = Some(now);
        self.step(now)
    }

    fn catch_up(&mut self) {
        let before = self.tree.layers.time();
        self.tree.layers.advance(self.time());
        if self.tree.layers.time() > before {
            self.scope.run(|| self.tree.publish_motions());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn pin_clock(&mut self, now: f64) {
        self.clock = Some(now);
    }

    fn step(&mut self, now: f64) -> bool {
        let _update = trace_span!("update").entered();
        let animating = self.tree.layers.is_animating();
        self.tree.layers.advance(now);
        self.tree.tick_caret(now);
        self.tree.tick_selection(now);
        self.flush();
        self.publish();
        self.flush();
        self.settle_layout();
        let changed = self.paint();
        self.tree.place_surfaces(self.cache.scale);
        self.following = self.tree.following();
        animating || changed
    }

    fn settle_layout(&mut self) {
        self.layout();
        self.publish();
        self.flush();
        if self.layout() {
            self.tree.stats.relayouts += 1;
        }
        self.publish();
        self.flush();
    }

    pub fn flush(&mut self) {
        self.catch_up();
        poll_tasks();
        loop {
            self.scope.run(|| {
                batch(|| {
                    while let Some(patch) = self.queue.pop() {
                        let theme = self.tree.theme().map(|theme| (theme, None));
                        patch.apply(&mut BuildCx {
                            parent: self.tree.content(),
                            tree: &mut self.tree,
                            queue: &self.queue,
                            text: &mut self.text_system,
                            theme,
                        });
                        self.tree.stats.patches += 1;
                    }
                    self.tree.settle_focus();
                });
            });
            self.apply_zoom();
            if self.queue.is_empty() {
                break;
            }
        }
        self.tree.resolve_styles();
        self.tree.publish_bindings();
    }

    fn publish(&mut self) {
        let _publish = trace_span!("publish").entered();
        self.tree.publish();
    }

    fn layout(&mut self) -> bool {
        if !self.tree.layout(&mut self.text_system) {
            return false;
        }
        if self.pointer.is_some() {
            let _hover = trace_span!("hover").entered();
            self.tree
                .set_pointer(self.pointer, self.modifiers.get_untracked());
        }
        true
    }

    fn paint(&mut self) -> bool {
        self.tree.paint(&mut self.cache, &mut self.text_system)
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.tree.set_window_visible(visible);
        self.tree.place_surfaces(self.cache.scale);
    }

    pub fn has_surfaces(&self) -> bool {
        self.tree.layers.has_surfaces()
    }

    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.window_handle.appearance.set_if_changed(appearance);
    }

    pub fn set_route(&mut self, route: impl Into<String>) {
        self.window_handle.route.set_if_changed(route.into());
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.window_handle.focused.set_if_changed(focused);
    }

    pub fn take_window_commands(&mut self) -> Vec<WindowCommand> {
        let mut commands = Vec::new();
        for sent in self.queue.take_sent() {
            match sent {
                Sent::Window(command) => commands.push(command),
                Sent::Copy(text) => self.tree.copy_text(&text),
            }
        }
        commands
    }

    pub fn add_font(&mut self, data: impl Into<Cow<'static, [u8]>>) -> Vec<String> {
        self.text_system.add_font(data)
    }

    pub fn next_frame(&self) -> Wake {
        if !self.queue.is_empty() || self.tree.dirty() != Dirty::None || self.following {
            return Wake::Now;
        }
        let completions = self
            .tree
            .layers
            .completion_deadline()
            .map_or(Wake::Idle, Wake::At);
        let now = self.time();
        self.tree
            .caret_wake(now)
            .sooner(completions)
            .sooner(self.tree.selection_wake(now))
    }

    pub fn is_animating(&self) -> bool {
        self.tree.layers.is_animating()
    }

    pub fn set_size(&mut self, size: Size) {
        self.viewport = size;
        self.rescale();
    }

    pub fn set_safe_area(&mut self, insets: Edges) {
        self.insets = insets;
        self.rescale();
    }

    pub fn set_input(&mut self, input: InputKind) {
        self.presentation.set_if_changed(Presentation {
            input,
            ..self.presentation.get_untracked()
        });
    }

    pub fn set_device_scale(&mut self, scale: f32) {
        self.device_scale = scale;
        self.rescale();
    }

    fn apply_zoom(&mut self) {
        let zoom = self.zoom.get_untracked();
        let zoom = if zoom.is_finite() && zoom > 0.0 {
            zoom
        } else {
            1.0
        };
        if zoom == self.zoomed {
            return;
        }
        let ratio = self.zoomed / zoom;
        self.pointer = self
            .pointer
            .map(|pointer| Point::new(pointer.x * ratio, pointer.y * ratio));
        self.zoomed = zoom;
        self.rescale();
    }

    fn rescale(&mut self) {
        let scale = self.device_scale * self.zoomed;
        if self.cache.scale != scale {
            self.cache.scale = scale;
            self.tree.invalidate_paint();
        }
        let size = Size::new(
            self.viewport.width / self.zoomed,
            self.viewport.height / self.zoomed,
        );
        self.size.set_if_changed(size);
        self.presentation.set_if_changed(Presentation {
            width: WidthClass::of(size),
            ..self.presentation.get_untracked()
        });
        self.tree.set_size(size);
        self.safe_area.set_if_changed(Edges {
            top: self.insets.top / self.zoomed,
            right: self.insets.right / self.zoomed,
            bottom: self.insets.bottom / self.zoomed,
            left: self.insets.left / self.zoomed,
        });
    }

    pub fn clear_color(&self) -> Color {
        self.tree.root_background().unwrap_or_else(|| {
            self.scope
                .run(|| Theme::current().with_untracked(|palette| palette.canvas))
        })
    }

    pub fn scale(&self) -> f32 {
        self.cache.scale
    }

    pub fn time(&self) -> f64 {
        self.clock
            .unwrap_or_else(|| self.epoch.elapsed().as_secs_f64())
    }

    pub fn set_epoch(&mut self, epoch: Instant) {
        self.epoch = epoch;
    }

    pub fn commit(&mut self) -> Commit {
        let _commit = trace_span!("commit").entered();
        let clear = self.clear_color();
        let scale = self.cache.scale;
        Commit {
            scale: (self.committed_scale.replace(scale) != Some(scale)).then_some(scale),
            clear: (self.committed_clear.replace(clear) != Some(clear)).then_some(clear),
            atlas: self.cache.atlas.take_uploads(),
            surfaces: self.tree.take_surface_ops(),
            ..self.tree.layers.commit()
        }
    }

    pub fn handle(&mut self, event: CompositorEvent) {
        if let CompositorEvent::Surface(id, status) = event {
            self.tree.surface_status(id, status);
        }
    }

    pub fn stats(&self) -> FrameStats {
        FrameStats {
            text: self.text_system.stats(),
            reactive: stats(),
            ..self.tree.stats()
        }
    }

    pub fn bounds_of(&self, name: &str) -> Option<Rect> {
        self.bounds_of_all(name).into_iter().next()
    }

    pub fn bounds_of_all(&self, name: &str) -> Vec<Rect> {
        self.tree.bounds_of_all(name)
    }

    pub fn named_bounds(&self) -> Vec<(String, Rect)> {
        self.tree.named_bounds()
    }

    pub fn text_of(&self, name: &str) -> Option<String> {
        self.tree.text_of(name)
    }

    pub fn text_bounds(&self, needle: &str) -> Option<Rect> {
        self.tree.text_bounds(needle)
    }

    pub fn text_index_at(&self, position: Point) -> Option<usize> {
        self.tree
            .hit_test(position)
            .iter()
            .rev()
            .find_map(|id| self.tree.text_hit(*id, position))
            .map(|hit| hit.index)
    }

    pub fn bounds_at(&self, position: Point) -> Option<Rect> {
        let hit = self.tree.hit_test(position);
        hit.last().map(|id| self.tree.window_bounds(*id))
    }

    pub fn text_cursor_area(&self) -> Option<Rect> {
        self.tree.caret_area()
    }

    pub fn cursor(&self) -> Cursor {
        self.tree.cursor()
    }

    pub fn has_focus(&self) -> bool {
        self.tree.focused().is_some()
    }

    pub fn accepts_text(&self) -> bool {
        let path = self.tree.focus_path();
        self.tree.focused().is_some() && !self.tree.listeners(&path, On::Ime).is_empty()
    }

    pub fn binding_for<T: Any>(&self) -> Option<Chord> {
        self.tree
            .effective_bindings()
            .into_iter()
            .find(|(kind, _)| *kind == TypeId::of::<T>())
            .map(|(_, chord)| chord)
    }

    fn emit(&mut self, listeners: &Listeners, event: Event<'_>) {
        self.scope.run(|| emit(listeners, event, |_| {}));
        self.flush();
    }

    fn refresh_pointer(&mut self) {
        self.flush();
        self.tree
            .set_pointer(self.pointer, self.modifiers.get_untracked());
        self.flush();
    }

    pub fn pointer_moved(&mut self, position: Point) {
        self.pointer = Some(position);
        self.outside = false;
        self.refresh_pointer();
        let path = self.tree.pointer_path().to_vec();
        let button = self.held.unwrap_or(PointerButton::Primary);
        self.dispatch(&path, On::PointerMove, position, button);
        if self.held == Some(PointerButton::Primary) {
            self.tree.move_selection(position, DRAG_SLOP);
        }
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

    pub fn clear_focus(&mut self) {
        self.flush();
        if self.tree.is_composing() {
            self.ime(ImeEvent::Preedit {
                text: String::new(),
                cursor: None,
            });
        }
        self.tree.set_focus(None, false);
        self.flush();
    }

    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        if self.modifiers.get_untracked() != modifiers {
            self.modifiers.set(modifiers);
            self.refresh_pointer();
        }
    }

    pub fn focus_next(&mut self, reverse: bool) -> bool {
        self.flush();
        let moved = self.tree.focus_next(reverse);
        self.flush();
        moved
    }

    pub fn dispatch_action<T: Any>(&mut self, action: T) -> bool {
        self.dispatch_action_ref(&action)
    }

    pub(crate) fn dispatch_action_ref(&mut self, action: &dyn Any) -> bool {
        self.flush();
        let path = self.tree.focus_path();
        let handlers = self.tree.action_handlers(&path, action.type_id());
        if handlers.is_empty()
            && let Some(action) = action.downcast_ref::<EditAction>()
            && self.tree.select_edit(&path, *action)
        {
            self.flush();
            return true;
        }
        self.invoke_action(action, &handlers)
    }

    fn invoke_action(&mut self, action: &dyn Any, handlers: &[ActionHandler]) -> bool {
        self.invoke(|| handlers.iter().for_each(|handler| handler(action)));
        !handlers.is_empty()
    }

    pub(crate) fn invoke(&mut self, f: impl FnOnce()) {
        self.scope.run(|| batch(f));
        self.flush();
    }

    pub fn key_down(&mut self, event: KeyEvent) -> bool {
        self.flush();
        if event.key == Key::Enter && self.tree.is_composing() {
            return true;
        }
        if self.key_event(&event, On::KeyDown) {
            return true;
        }
        let path = self.tree.focus_path();
        if let Some((action, handlers)) = self.tree.binding(&path, &event) {
            return self.invoke_action(&*action, &handlers);
        }
        let selected = if event.key == Key::Escape {
            self.tree.escape_selection(&path)
        } else {
            EditAction::from_key(&event).is_some_and(|action| self.tree.select_edit(&path, action))
        };
        if selected {
            self.flush();
            return true;
        }
        let shortcut = self
            .shortcuts
            .iter()
            .find(|(chord, _)| chord.matches(&event))
            .map(|(_, item)| item.clone());
        if let Some(item) = shortcut {
            return item.dispatch(self);
        }
        let Modifiers {
            shift,
            ctrl,
            alt,
            meta,
        } = event.modifiers;
        event.key == Key::Tab && !(ctrl || alt || meta) && self.focus_next(shift)
    }

    pub fn bind_menus(&mut self, menus: &[Menu]) {
        self.shortcuts = MenuItem::shortcuts(menus);
    }

    #[track_caller]
    pub fn press_chord(&mut self, chord: impl IntoChord) -> bool {
        let Chord { key, modifiers } = chord.into_chord();
        let typed = match &key {
            Key::Character(text) if modifiers.shift => Some(text.to_uppercase()),
            Key::Character(text) => Some(text.clone()),
            Key::Space => Some(" ".to_owned()),
            _ => None,
        };
        let mut down = KeyEvent::new(key.clone()).with_modifiers(modifiers);
        if let Some(typed) = typed.filter(|_| !(modifiers.ctrl || modifiers.alt || modifiers.meta))
        {
            down = down.with_text(typed);
        }
        let handled = self.key_down(down);
        self.key_up(KeyEvent::new(key).with_modifiers(modifiers));
        handled
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
        self.flush();
        let path = self.tree.focus_path();
        let listeners = self.tree.listeners(&path, on);
        self.emit(&listeners, event);
    }

    pub fn pointer_down(&mut self, button: PointerButton) {
        let Some(position) = self.pointer else { return };
        self.held = Some(button);
        let now = self.time();
        self.flush();
        if button == PointerButton::Primary {
            self.tree.press();
            self.tree.focus_pointer();
            let repeat = self.clicks.filter(|(at, last, _)| {
                let travel = position - *last;
                now - at <= MULTI_CLICK_SECONDS && travel.x.hypot(travel.y) <= MULTI_CLICK_SLOP
            });
            self.clicks = Some((now, position, repeat.map_or(1, |(_, _, count)| count + 1)));
        }
        let path = self.tree.hover_path().to_vec();
        self.dispatch(&path, On::PointerDown, position, button);
        if button == PointerButton::Primary {
            self.tree.press_selection(
                &path,
                position,
                self.clicks.map_or(1, |(_, _, count)| count),
                self.modifiers.get_untracked().shift,
            );
            let samples = Samples::new(self.time(), position);
            let targets = self.tree.listeners(&path, On::Drag);
            self.drag = targets.first().map(|(element, _)| Drag {
                element: *element,
                axis: self.tree.drag_axis(*element),
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
        self.flush();
        let path = self.tree.pointer_path().to_vec();
        self.dispatch(&path, On::PointerUp, position, button);
        let selected = primary && self.tree.release_selection();
        if selected {
            self.clicks = None;
        }
        self.tree.release_capture();
        self.refresh_pointer();
        let clicked = if primary {
            self.tree.release()
        } else {
            Vec::new()
        };
        if dragged.is_none() && !selected {
            self.dispatch(&clicked, On::Click, position, button);
        }
        if self.outside {
            self.pointer_left();
        }
    }

    pub fn pointer_cancel(&mut self) {
        self.release_press();
        self.pointer_left();
    }

    fn release_press(&mut self) {
        self.held = None;
        self.tree.release_selection();
        if let (Some(drag), Some(position)) =
            (self.drag.take().filter(|drag| drag.started), self.pointer)
        {
            self.drag_event(&drag, DragPhase::End, position);
        }
        self.tree.release_capture();
        self.tree.release();
    }

    pub fn touch(&mut self, phase: TouchPhase, position: Point) {
        match phase {
            TouchPhase::Start => {
                if self.touch.is_some() {
                    self.touch(TouchPhase::Cancel, position);
                }
                self.pointer_moved(position);
                self.pointer_down(PointerButton::Primary);
                self.touch = Some(Touch {
                    start: position,
                    last: position,
                    panning: false,
                });
            }
            TouchPhase::Move => {
                let Some(mut touch) = self.touch.take() else {
                    return;
                };
                let travel = position - touch.start;
                let distance = travel.x.hypot(travel.y);
                let yields = self.drag.as_ref().is_some_and(|drag| {
                    !drag.started
                        && distance >= DRAG_SLOP
                        && drag.axis.is_some_and(|axis| {
                            !axis.follows(travel)
                                || self.scroll_target(Point::ZERO - travel, false).is_some()
                        })
                });
                if yields {
                    self.drag = None;
                }
                if touch.panning {
                    self.scroll_drag(position - touch.last);
                } else if self.drag.is_none() && distance >= TOUCH_SLOP {
                    touch.panning = true;
                    self.release_press();
                    self.scroll_drag(travel);
                } else {
                    self.pointer_moved(position);
                }
                touch.last = position;
                self.touch = Some(touch);
            }
            TouchPhase::End | TouchPhase::Cancel => {
                let Some(touch) = self.touch.take() else {
                    return;
                };
                if touch.panning {
                    self.scroll_release();
                } else if phase == TouchPhase::End {
                    if position != touch.last {
                        self.pointer_moved(position);
                    }
                    self.pointer_up(PointerButton::Primary);
                } else {
                    self.release_press();
                }
                self.pointer_left();
            }
        }
    }

    pub fn file_drop(&mut self, phase: FileDropPhase, paths: Vec<PathBuf>) -> bool {
        self.flush();
        let position = self.pointer;
        let path = match position {
            Some(_) => self.tree.hover_path().to_vec(),
            None => self.tree.focus_path(),
        };
        let listeners = self.tree.listeners(&path, On::FileDrop);
        let event = FileDropEvent::new(phase, position, paths);
        self.emit(&listeners, Event::FileDrop(&event));
        event.stopped.get()
    }

    pub fn scroll(&mut self, delta: Point) -> bool {
        if self.pointer.is_none() {
            return false;
        }
        let by = Point::ZERO - delta;
        let handled = if !self.scroll_event(delta) {
            true
        } else if let Some(layer) = self.scroll_target(by, false) {
            self.tree.scroll_by(layer, by);
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
            let now = self.time();
            let layer = (self.scrolling.as_ref().and_then(|drag| drag.layer))
                .or_else(|| self.scroll_target(by, true));
            let drag = self.scrolling.get_or_insert_with(|| ScrollDrag {
                layer: None,
                travel: Point::ZERO,
                samples: Samples::default(),
            });
            drag.layer = layer;
            drag.travel = drag.travel + by;
            drag.samples.push(now, drag.travel);
            if let Some(layer) = layer {
                self.tree.scroll_drag(layer, by);
            }
        }
        self.refresh_pointer();
    }

    pub fn scroll_release(&mut self) {
        let Some(drag) = self.scrolling.take() else {
            return;
        };
        let velocity = drag.samples.velocity(self.time());
        self.catch_up();
        if let Some(layer) = drag.layer {
            self.tree.scroll_release(layer, velocity);
        }
    }

    fn scroll_event(&mut self, delta: Point) -> bool {
        let Some(position) = self.pointer else {
            return false;
        };
        self.flush();
        let listeners = self.tree.listeners(self.tree.hover_path(), On::Scroll);
        let event = ScrollEvent::new(position, delta);
        self.emit(&listeners, Event::Scroll(&event));
        !event.stopped.get()
    }

    fn scroll_target(&self, by: Point, elastic: bool) -> Option<LayerId> {
        let targets = self.tree.scroll_targets();
        let moves = |layer: &&LayerId| {
            self.tree
                .layers
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
        let now = self.time();
        let Some(drag) = &mut self.drag else { return };
        drag.samples.push(now, position);
        let travel = position - drag.start;
        if !drag.started && travel.x.hypot(travel.y) < DRAG_SLOP {
            return;
        }
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        if !drag.started && drag.axis.is_some_and(|axis| !axis.follows(travel)) {
            return;
        }
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
        self.flush();
        let listeners = self.tree.listeners(&[drag.element], On::Drag);
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
            velocity: drag.samples.velocity(self.time()),
        };
        self.emit(&listeners, Event::Drag(&event));
    }

    fn dispatch(&mut self, path: &[ElementId], on: On, position: Point, button: PointerButton) {
        self.flush();
        if on == On::Click {
            self.tree
                .text_click(path, position, self.modifiers.get_untracked());
            self.flush();
        }
        let listeners = self.tree.listeners(path, on);
        let mut event = PointerEvent::new(position, button);
        event.modifiers = self.modifiers.get_untracked();
        event.click_count = self.clicks.map_or(1, |(_, _, count)| count);
        let tree = &self.tree;
        let target = Cell::new(None);
        self.scope.run(|| {
            emit(&listeners, Event::Pointer(&event), |element| {
                if !event.captured.get() {
                    target.set(Some(element));
                }
                event.local.set(tree.to_local(element, position));
                event.size.set(tree.size_of(element));
            });
        });
        let captured = target.take().filter(|_| event.captured.get());
        if let Some(element) = captured.filter(|_| on == On::PointerDown) {
            self.tree.capture(path, element);
        }
        self.flush();
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        self.scope.dispose();
    }
}
