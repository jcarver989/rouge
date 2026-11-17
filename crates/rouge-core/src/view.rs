use crate::animation::{Keyframes, LayerProp, LayoutTransition, Timing};
use crate::canvas::{PaintOp, TextHit};
use crate::clipboard::{ClipboardTarget, Pasted};
use crate::compositor::{EffectSlot, MotionId, ScrollLinked, ScrollState};
use crate::event::{
    Cursor, DragEvent, FileDropEvent, FocusEvent, ImeEvent, IntoChord, Key, KeyEvent, Modifiers,
    PointerEvent, ScrollEvent,
};
use crate::focus::FocusHandle;
use crate::motion::{MotionOp, MotionProp};
use crate::prop::{AnyEffect, IntoProp, Prop, PropFrom};
use crate::scroll::{Scrollable, scroll_key};
use crate::style::{Declaration, DelegateStyle, StyleDelta, StyleState, Styled};
use crate::text::{SpanStyle, TextAlign, TextStyle, TextSystem, WrapIndent};
use crate::theme::{Fill, Theme};
use crate::tree::edit::EditOp;
use crate::tree::{
    ElementId, ElementSpec, Event, FocusSpec, Interaction, Kind, Node, On, STATES, Tree, Trigger,
};
use crate::window::Sent;
use rouge_macros::component;
use rouge_reactive::{Scope, Signal, effect, provide_context, run_in, use_context};
use rouge_render::{BlendMode, EffectInstance, ImageData, Mask, Point, Rect, Size};
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::mem::{replace, take};
use std::ops::Range;
use std::rc::Rc;
use taffy::Overflow;

pub(crate) enum TextOp {
    Spans(Vec<(Range<usize>, SpanStyle)>),
    Placeholder(String),
    PlaceholderColor(Fill),
}

pub(crate) enum Op {
    Style(Declaration),
    Variant(usize, usize, Declaration),
    Text(TextOp),
    Content(String),
    Cursor(Option<Cursor>),
    State(Interaction, bool),
    Condition(usize, bool),
    LayerProp(LayerProp, MotionProp, bool),
    Effect(EffectSlot, EffectInstance),
    ScrollLink(u64, ScrollLinked),
    ScrollTo(Point, bool),
}

impl Op {
    pub(crate) fn apply(self, id: ElementId, tree: &mut Tree) {
        match self {
            Op::Style(declaration) => tree.update_style(id, declaration),
            Op::Variant(slot, index, declaration) => {
                tree.update_variant(id, slot, index, declaration);
            }
            Op::Text(op) => tree.update_text_prop(id, op),
            Op::Content(content) => tree.set_text_content(id, content),
            Op::Cursor(cursor) => tree.set_cursor(id, cursor),
            Op::State(which, on) => tree.set_state(id, which, on),
            Op::Condition(index, on) => tree.set_condition(id, index, on),
            Op::LayerProp(prop, value, animate) => tree.set_layer_prop(id, prop, value, animate),
            Op::Effect(slot, effect) => tree.set_layer_effect(id, slot, effect),
            Op::ScrollLink(key, link) => tree.set_scroll_link(id, key, link),
            Op::ScrollTo(offset, anchored) => tree.scroll_to(id, offset, anchored),
        }
    }
}

enum Write {
    Now(Op),
    Live(Box<dyn FnMut() -> Option<Op>>),
}

pub(crate) enum Patch {
    Build(Box<dyn FnOnce(&mut BuildCx<'_>)>),
    Element(ElementId, Op),
    Image(ElementId, Option<ImageData>),
    Canvas(ElementId, Vec<PaintOp>),
    Edit(ElementId, EditOp),
    Paste(ElementId, ClipboardTarget, Pasted),
    Autofocus(ElementId),
    Focus(u64, bool),
    Motion(MotionId, MotionOp),
}

impl Patch {
    pub(crate) fn apply(self, cx: &mut BuildCx<'_>) {
        match self {
            Patch::Build(build) => build(cx),
            Patch::Element(id, op) => op.apply(id, cx.tree),
            Patch::Image(id, data) => cx.tree.set_image(id, data),
            Patch::Canvas(id, ops) => cx.tree.set_canvas(id, ops, cx.text),
            Patch::Edit(id, op) => cx.tree.edit(id, op, cx.text),
            Patch::Paste(id, target, pasted) => cx.tree.paste(id, &target, pasted, cx.text),
            Patch::Autofocus(id) => cx.tree.autofocus(id),
            Patch::Focus(handle, focus) => cx.tree.focus_handle(handle, focus),
            Patch::Motion(id, op) => cx.tree.motion(id, op),
        }
    }
}

#[derive(Default)]
struct Pending {
    patches: VecDeque<Patch>,
    sent: Vec<Sent>,
}

#[derive(Clone, Default)]
pub(crate) struct Queue(Rc<RefCell<Pending>>);

impl Queue {
    pub(crate) fn push(&self, patch: Patch) {
        self.0.borrow_mut().patches.push_back(patch);
    }

    pub(crate) fn build(&self, build: impl FnOnce(&mut BuildCx<'_>) + 'static) {
        self.push(Patch::Build(Box::new(build)));
    }

    pub(crate) fn pop(&self) -> Option<Patch> {
        self.0.borrow_mut().patches.pop_front()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.borrow().patches.is_empty()
    }

    pub(crate) fn post(&self, sent: Sent) {
        self.0.borrow_mut().sent.push(sent);
    }

    pub(crate) fn take_sent(&self) -> Vec<Sent> {
        take(&mut self.0.borrow_mut().sent)
    }
}

pub struct BuildCx<'a> {
    pub(crate) tree: &'a mut Tree,
    pub(crate) queue: &'a Queue,
    pub(crate) text: &'a mut TextSystem,
    pub(crate) parent: Node,
    pub(crate) theme: Option<(Theme, Option<Scope>)>,
}

impl BuildCx<'_> {
    fn current_theme(&mut self) -> Option<Theme> {
        let scope = Scope::current();
        match self.theme {
            Some((theme, seen)) if seen == scope => return Some(theme),
            _ => {}
        }
        let theme = use_context::<Theme>()?;
        self.theme = Some((theme, scope));
        Some(theme)
    }
}

impl BuildCx<'_> {
    pub(crate) fn within(&mut self, parent: Node, build: impl FnOnce(&mut Self)) {
        let outer = replace(&mut self.parent, parent);
        build(self);
        self.parent = outer;
    }
}

pub trait View: 'static {
    fn build(self, cx: &mut BuildCx<'_>);

    fn into_any(self) -> AnyView
    where
        Self: Sized,
    {
        AnyView(Box::new(move |cx| self.build(cx)))
    }
}

pub struct AnyView(Box<dyn FnOnce(&mut BuildCx<'_>)>);

impl View for AnyView {
    fn build(self, cx: &mut BuildCx<'_>) {
        (self.0)(cx);
    }

    fn into_any(self) -> AnyView {
        self
    }
}

impl<T: View> View for Vec<T> {
    fn build(self, cx: &mut BuildCx<'_>) {
        for view in self {
            view.build(cx);
        }
    }
}

impl View for () {
    fn build(self, _: &mut BuildCx<'_>) {}
}

impl<T: View> View for Option<T> {
    fn build(self, cx: &mut BuildCx<'_>) {
        if let Some(view) = self {
            view.build(cx);
        }
    }
}

pub trait Apply: Sized {
    fn when(self, condition: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if condition { f(self) } else { self }
    }

    fn when_some<T>(self, value: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self {
        match value {
            Some(value) => f(self, value),
            None => self,
        }
    }
}

impl<T> Apply for T {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusPolicy {
    #[default]
    Tab,
    Roving,
    None,
}

#[derive(Default)]
pub struct Div {
    pub(crate) spec: ElementSpec,
    writes: Vec<Write>,
    children: Vec<AnyView>,
    provided: Vec<Box<dyn FnOnce()>>,
    effects: usize,
    autofocus: bool,
    focus_policy: FocusPolicy,
}

#[component]
pub fn div() -> Div {
    Div::default()
}

impl Div {
    pub fn preserve_focus(mut self) -> Self {
        self.spec.flags.preserve_focus = true;
        self
    }

    pub fn root_host(mut self) -> Self {
        self.spec.flags.root_host = true;
        self
    }

    pub fn group(mut self, name: impl Into<String>) -> Self {
        self.spec.extras().group = Some(name.into());
        self
    }

    pub fn when<U>(
        mut self,
        condition: impl IntoProp<bool, U>,
        f: impl FnOnce(StyleDelta) -> StyleDelta,
    ) -> Self {
        let triggers = &mut self.spec.extras().triggers;
        triggers.push((Trigger::When(false), Vec::new()));
        let index = triggers.len() - 1;
        self.variant(STATES + index, f(StyleDelta::default()))
            .bind(condition, move |on| Op::Condition(index, on))
    }

    fn variant(mut self, slot: usize, delta: StyleDelta) -> Self {
        assert!(
            delta.states.is_empty(),
            "rouge-core: a state or condition style cannot nest another state style"
        );
        let offset = self
            .spec
            .extras()
            .variant_mut(slot)
            .map_or(0, |variant| variant.len());
        let mut declarations = Vec::with_capacity(delta.declarations.len());
        for (index, declaration) in (offset..).zip(delta.declarations) {
            let compute = match declaration {
                Prop::Static(declaration) => {
                    declarations.push(declaration);
                    continue;
                }
                Prop::Dynamic(compute) => compute,
            };
            declarations.push(Declaration::Invisible(false));
            self = self.write(Prop::Dynamic(compute), move |declaration| {
                Some(Op::Variant(slot, index, declaration))
            });
        }
        if let Some(variant) = self.spec.extras().variant_mut(slot) {
            variant.extend(declarations);
        }
        self
    }

    pub fn test_id(mut self, name: impl Into<String>) -> Self {
        self.spec.extras().test_id = Some(name.into());
        self
    }

    pub fn test_scope(mut self, name: impl ToString) -> Self {
        let extras = self.spec.extras();
        extras.test_id = Some(name.to_string());
        extras.test_scope = true;
        self
    }

    fn bind<T: 'static, U>(
        self,
        value: impl IntoProp<T, U>,
        mut op: impl FnMut(T) -> Op + 'static,
    ) -> Self {
        self.write(value.into_prop(), move |value| Some(op(value)))
    }

    fn write<T: 'static>(
        mut self,
        value: Prop<T>,
        mut op: impl FnMut(T) -> Option<Op> + 'static,
    ) -> Self {
        let write = match value {
            Prop::Static(value) => match op(value) {
                Some(op) => Write::Now(op),
                None => return self,
            },
            Prop::Dynamic(compute) => Write::Live(Box::new(move || op(compute()))),
        };
        self.writes.push(write);
        self
    }

    fn layer_prop<T>(self, prop: LayerProp, value: impl IntoProp<MotionProp, T>) -> Self {
        let mut last = None;
        self.layer().write(value.into_prop(), move |value| {
            let previous = last.replace(value);
            (previous != Some(value)).then_some(Op::LayerProp(prop, value, previous.is_some()))
        })
    }

    fn apply_effect<T: 'static, U>(self, slot: EffectSlot, effect: impl IntoProp<T, U>) -> Self
    where
        AnyEffect: PropFrom<T>,
    {
        self.layer().bind(effect, move |effect| {
            Op::Effect(slot, AnyEffect::prop_from(effect).0)
        })
    }

    pub fn scroll_linked<T>(
        self,
        state: Signal<ScrollState>,
        link: impl IntoProp<ScrollLinked, T>,
    ) -> Self {
        let key = scroll_key(state);
        self.layer()
            .bind(link, move |link| Op::ScrollLink(key, link))
    }

    pub(crate) fn scrollable(mut self) -> Self {
        self.spec.extras().scroll.get_or_insert_default();
        self
    }

    pub fn provide<T: Clone + 'static>(mut self, value: T) -> Self {
        self.provided.push(Box::new(move || provide_context(value)));
        self
    }

    pub fn child(mut self, child: impl View) -> Self {
        self.children.push(child.into_any());
        self
    }

    pub fn children<T: View>(mut self, children: impl IntoIterator<Item = T>) -> Self {
        self.children
            .extend(children.into_iter().map(View::into_any));
        self
    }

    #[track_caller]
    pub fn on_click(self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.listen(On::Click, move |event| {
            if let Event::Pointer(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_pointer_down(self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.listen(On::PointerDown, move |event| {
            if let Event::Pointer(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_pointer_up(self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.listen(On::PointerUp, move |event| {
            if let Event::Pointer(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_pointer_move(self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.listen(On::PointerMove, move |event| {
            if let Event::Pointer(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_key_down(self, f: impl Fn(&KeyEvent) + 'static) -> Self {
        self.listen(On::KeyDown, move |event| {
            if let Event::Key(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_key_up(self, f: impl Fn(&KeyEvent) + 'static) -> Self {
        self.listen(On::KeyUp, move |event| {
            if let Event::Key(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_focus(self, f: impl Fn(&FocusEvent) + 'static) -> Self {
        self.listen(On::Focus, move |event| {
            if let Event::Focus(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_blur(self, f: impl Fn(&FocusEvent) + 'static) -> Self {
        self.listen(On::Blur, move |event| {
            if let Event::Focus(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_hover(self, f: impl Fn(bool) + 'static) -> Self {
        self.listen(On::Hover, move |event| {
            if let Event::Hover(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_ime(self, f: impl Fn(&ImeEvent) + 'static) -> Self {
        self.listen(On::Ime, move |event| {
            if let Event::Ime(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_drag(self, f: impl Fn(&DragEvent) + 'static) -> Self {
        self.listen(On::Drag, move |event| {
            if let Event::Drag(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_scroll(self, f: impl Fn(&ScrollEvent) + 'static) -> Self {
        self.listen(On::Scroll, move |event| {
            if let Event::Scroll(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    pub fn on_file_drop(self, f: impl Fn(&FileDropEvent) + 'static) -> Self {
        self.listen(On::FileDrop, move |event| {
            if let Event::FileDrop(event) = *event {
                f(event);
            }
        })
    }

    #[track_caller]
    fn listen(mut self, on: On, listener: impl Fn(&Event<'_>) + 'static) -> Self {
        let owner = Scope::current();
        let listener = move |event: &Event<'_>| {
            run_in(owner, || listener(event));
        };
        self.spec.extras().listeners.push((on, Rc::new(listener)));
        self
    }

    pub fn translate_x<T>(self, value: impl IntoProp<MotionProp, T>) -> Self {
        self.layer_prop(LayerProp::TranslateX, value)
    }

    pub fn translate_y<T>(self, value: impl IntoProp<MotionProp, T>) -> Self {
        self.layer_prop(LayerProp::TranslateY, value)
    }

    pub fn scale<T>(self, value: impl IntoProp<MotionProp, T>) -> Self {
        self.layer_prop(LayerProp::Scale, value)
    }

    pub fn rotate<T>(self, value: impl IntoProp<MotionProp, T>) -> Self {
        self.layer_prop(LayerProp::Rotate, value)
    }

    pub fn opacity<T>(self, value: impl IntoProp<MotionProp, T>) -> Self {
        self.layer_prop(LayerProp::Opacity, value)
    }

    pub fn on_activate(self, f: impl Fn() + 'static) -> Self {
        let clicked = Rc::new(f);
        let pressed = clicked.clone();
        let focus = self.focus_policy;
        self.unselectable()
            .cursor(Cursor::Pointer)
            .on_click(move |event| {
                event.stop_propagation();
                clicked();
            })
            .on_key_down(move |event| {
                if matches!(event.key, Key::Enter | Key::Space) {
                    event.stop_propagation();
                    pressed();
                }
            })
            .focus_policy(focus)
    }

    #[track_caller]
    pub fn on_double_click(self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.on_click(move |event| {
            if event.click_count == 2 {
                f(event);
            }
        })
    }

    pub fn focus_policy(mut self, policy: FocusPolicy) -> Self {
        self.focus_policy = policy;
        let extras = self.spec.extras();
        extras.focus = match policy {
            FocusPolicy::Tab => Some(extras.focus.unwrap_or_default()),
            FocusPolicy::Roving => Some(FocusSpec {
                tab_index: -1,
                ..extras.focus.unwrap_or_default()
            }),
            FocusPolicy::None => None,
        };
        self
    }

    pub fn is_selected<U>(self, selected: impl IntoProp<bool, U>) -> Self {
        self.state(Interaction::Selected, selected)
    }

    pub fn is_checked<U>(self, checked: impl IntoProp<bool, U>) -> Self {
        self.state(Interaction::Checked, checked)
    }

    fn state<U>(self, which: Interaction, on: impl IntoProp<bool, U>) -> Self {
        self.bind(on, move |on| Op::State(which, on))
    }

    pub fn enabled<U>(self, enabled: impl IntoProp<bool, U>) -> Self {
        self.bind(enabled, |enabled| {
            Op::State(Interaction::Disabled, !enabled)
        })
    }

    pub fn focusable(mut self) -> Self {
        self.spec.extras().focus.get_or_insert_default();
        self
    }

    pub fn tab_index(mut self, index: i32) -> Self {
        self.spec.extras().focus.get_or_insert_default().tab_index = index;
        self
    }

    pub fn track_focus(mut self, handle: FocusHandle) -> Self {
        self.spec.extras().focus.get_or_insert_default().handle = Some(handle.binding());
        self
    }

    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self.focusable()
    }

    pub fn focus_scope(mut self) -> Self {
        self.spec.flags.focus_scope = true;
        self
    }

    pub fn pass_through(mut self) -> Self {
        self.spec.flags.pass_through = true;
        self
    }

    pub fn cursor<U>(self, cursor: impl IntoProp<Option<Cursor>, U>) -> Self {
        self.bind(cursor, Op::Cursor)
    }

    pub fn track_size(mut self, size: Signal<Size>) -> Self {
        self.spec.extras().size_observer = Some(size);
        self
    }

    pub fn track_bounds(mut self, bounds: Signal<Rect>) -> Self {
        self.spec.extras().bounds_observer = Some(bounds);
        self
    }

    pub fn track_paint_region(mut self, region: Signal<crate::PaintRegion>) -> Self {
        self.spec.extras().paint_region = Some(region);
        self
    }

    #[track_caller]
    pub fn bind_key<T: Any>(mut self, chord: impl IntoChord, action: T) -> Self {
        self.spec
            .extras()
            .bindings
            .push((chord.into_chord(), Rc::new(action)));
        self
    }

    #[track_caller]
    pub fn on_action<T: Any>(mut self, f: impl Fn(&T) + 'static) -> Self {
        let owner = Scope::current();
        let handler = move |action: &dyn Any| {
            if let Some(action) = action.downcast_ref::<T>() {
                run_in(owner, || f(action));
            }
        };
        self.spec
            .extras()
            .actions
            .push((TypeId::of::<T>(), Rc::new(handler)));
        self
    }

    pub fn layer(mut self) -> Self {
        self.spec.layer.get_or_insert_default();
        self
    }

    pub fn rasterize(mut self) -> Self {
        self.spec.layer.get_or_insert_default().rasterize = true;
        self
    }

    pub fn effect<T>(mut self, effect: impl IntoProp<AnyEffect, T>) -> Self {
        let slot = EffectSlot::Content(self.effects);
        self.effects += 1;
        self.apply_effect(slot, effect)
    }

    pub fn backdrop<T>(self, effect: impl IntoProp<AnyEffect, T>) -> Self {
        self.apply_effect(EffectSlot::Backdrop, effect)
    }

    pub fn mask<T>(self, mask: impl IntoProp<Mask, T>) -> Self {
        self.apply_effect(EffectSlot::Mask, mask)
    }

    pub fn blend<T>(self, mode: impl IntoProp<BlendMode, T>) -> Self {
        self.apply_effect(EffectSlot::Blend, mode)
    }

    pub fn transition(mut self, timing: impl Into<Timing>) -> Self {
        self.spec.layer.get_or_insert_default().transition = Some(timing.into());
        self
    }

    pub fn layout_transition(mut self, transition: impl Into<LayoutTransition>) -> Self {
        self.spec.layer.get_or_insert_default().layout_transition = Some(transition.into());
        self
    }

    pub fn animate(mut self, keyframes: Keyframes) -> Self {
        self.spec.layer.get_or_insert_default().keyframes = Some(keyframes);
        self
    }
}

impl Scrollable for Div {
    fn scroll_state(mut self, state: Signal<ScrollState>) -> Self {
        self.spec.extras().scroll.get_or_insert_default().observer = Some(state);
        self.write(
            Prop::Dynamic(Box::new(move || {
                let state = state.get();
                (state.offset, state.anchored)
            })),
            |(offset, anchored)| Some(Op::ScrollTo(offset, anchored)),
        )
    }
}

impl Styled for Div {
    fn declare(self, declaration: Prop<Declaration>) -> Self {
        self.write(declaration, |declaration| Some(Op::Style(declaration)))
    }

    fn restyle(mut self, state: StyleState, delta: StyleDelta) -> Self {
        let slot = match state {
            StyleState::Interaction(interaction) => interaction as usize,
            StyleState::GroupHover(name) => {
                let triggers = &mut self.spec.extras().triggers;
                triggers.push((Trigger::GroupHover(name), Vec::new()));
                STATES + triggers.len() - 1
            }
        };
        self.variant(slot, delta)
    }
}

impl View for Div {
    fn build(self, cx: &mut BuildCx<'_>) {
        self.mount(cx);
    }
}

impl Div {
    pub(crate) fn mount(mut self, cx: &mut BuildCx<'_>) -> ElementId {
        let inherited = cx.theme.map(|(theme, _)| theme);
        let theme = cx.current_theme();
        if let Some(theme) = theme.filter(|theme| Some(*theme) != inherited) {
            self.spec.extras().theme = Some(theme);
            cx.tree.watch_theme(theme);
        }
        let extras = self.spec.extras.as_deref();
        if extras.is_some_and(|extras| extras.scroll.is_some()) {
            let overflow = Declaration::Overflow(Overflow::Scroll);
            self.writes.push(Write::Now(Op::Style(overflow)));
        }
        let id = cx.tree.create_element(self.spec);
        let mut first: Option<Rc<RefCell<Vec<Op>>>> = None;
        for write in self.writes {
            match write {
                Write::Now(op) => op.apply(id, cx.tree),
                Write::Live(mut live) => {
                    let first = first.get_or_insert_default();
                    let (queue, mut pending) = (cx.queue.clone(), Some(first.clone()));
                    effect(move || {
                        let first = pending.take();
                        let Some(op) = live() else { return };
                        match first {
                            Some(first) => first.borrow_mut().push(op),
                            None => queue.push(Patch::Element(id, op)),
                        }
                    });
                    let ops = take(&mut *first.borrow_mut());
                    for op in ops {
                        op.apply(id, cx.tree);
                    }
                }
            }
        }
        cx.tree.resolve_new(id);
        if self.autofocus {
            cx.queue.push(Patch::Autofocus(id));
        }
        let outer = cx.theme;
        let (children, provided) = (self.children, self.provided);
        cx.within(Node::Element(id), |cx| {
            let build = |cx: &mut BuildCx<'_>| {
                for child in children {
                    child.build(cx);
                }
            };
            if provided.is_empty() {
                build(cx);
            } else {
                Scope::new().run(|| {
                    for provide in provided {
                        provide();
                    }
                    build(cx);
                });
            }
        });
        cx.theme = outer;
        cx.tree.append(cx.parent, id);
        id
    }
}

pub struct Text(pub(crate) Div);

#[component]
pub fn text<U>(children: impl IntoProp<String, U>) -> Text {
    let mut div = div();
    div.spec.kind = Kind::Text(Box::default());
    Text(div.bind(children, Op::Content))
}

#[component]
pub fn rich_text<T, U>(
    children: impl IntoProp<String, T>,
    spans: impl IntoProp<Vec<(Range<usize>, SpanStyle)>, U>,
) -> Text {
    text(children).spans(spans)
}

impl Text {
    pub fn test_id(self, name: impl Into<String>) -> Self {
        Text(self.0.test_id(name))
    }

    pub fn spans<U>(self, spans: impl IntoProp<Vec<(Range<usize>, SpanStyle)>, U>) -> Self {
        self.text_prop(spans, TextOp::Spans)
    }

    pub fn on_range_hover(self, f: impl Fn(Option<usize>) + 'static) -> Self {
        self.on_range_hover_with_modifiers(move |index, _| f(index))
    }

    pub fn on_range_hover_with_modifiers(
        mut self,
        f: impl Fn(Option<usize>, Modifiers) + 'static,
    ) -> Self {
        let owner = Scope::current();
        self.0.spec.extras().text_hover = Some(Rc::new(move |hit: Option<TextHit>, modifiers| {
            run_in(owner, || f(hit.map(|hit| hit.index), modifiers));
        }));
        self
    }

    pub fn on_range_click(self, f: impl Fn(Option<usize>) + 'static) -> Self {
        self.on_range_click_with_modifiers(move |index, _| f(index))
    }

    pub fn on_range_click_with_modifiers(
        mut self,
        f: impl Fn(Option<usize>, Modifiers) + 'static,
    ) -> Self {
        let owner = Scope::current();
        self.0.spec.extras().text_click = Some(Rc::new(move |hit: Option<TextHit>, modifiers| {
            run_in(owner, || f(hit.map(|hit| hit.index), modifiers));
        }));
        self
    }

    pub(crate) fn text_prop<T: 'static, U>(
        self,
        value: impl IntoProp<T, U>,
        op: fn(T) -> TextOp,
    ) -> Self {
        Text(self.0.bind(value, move |value| Op::Text(op(value))))
    }

    fn text_style(mut self, set: impl FnOnce(&mut TextStyle)) -> Self {
        if let Some(text) = self.0.spec.kind.text_mut() {
            set(&mut text.style);
        }
        self
    }

    pub fn text_size<U>(self, size: impl IntoProp<f32, U>) -> Self {
        Styled::text_size(self, size)
    }

    pub fn color<U>(self, color: impl IntoProp<Fill, U>) -> Self {
        self.text_color(color)
    }

    pub fn weight<U>(self, weight: impl IntoProp<f32, U>) -> Self {
        self.font_weight(weight)
    }

    pub fn bold(self) -> Self {
        self.font_weight(700.0)
    }

    pub fn align(self, align: TextAlign) -> Self {
        self.text_style(|style| style.align = align)
    }

    pub fn no_wrap(self) -> Self {
        self.text_style(|style| style.wrap = false)
    }

    pub fn wrap_indent(self, indent: WrapIndent) -> Self {
        self.text_style(|style| style.wrap_indent = indent)
    }

    pub fn tab_size(self, columns: u8) -> Self {
        self.text_style(|style| style.tab_size = columns)
    }

    pub fn cursor<U>(self, cursor: impl IntoProp<Option<Cursor>, U>) -> Self {
        Text(self.0.cursor(cursor))
    }
}

impl DelegateStyle for Text {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.0
    }
}

impl View for Text {
    fn build(self, cx: &mut BuildCx<'_>) {
        self.0.mount(cx);
    }
}
