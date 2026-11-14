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
use crate::style::{Declaration, Dirty, StyleDelta, Styled};
use crate::styled;
use crate::text::{SpanStyle, TextAlign, TextStyle, TextSystem, WrapIndent};
use crate::theme::Theme;
use crate::tree::edit::EditOp;
use crate::tree::{
    ElementId, ElementSpec, Event, Interaction, Kind, Node, On, STATES, TextData, Tree,
};
use crate::window::Sent;
use rouge_macros::component;
use rouge_reactive::{Scope, Signal, effect, run_in, untrack};
use rouge_render::{BlendMode, Color, EffectInstance, ImageData, Mask, Point, Rect, Size};
use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::mem::{replace, take};
use std::ops::Range;
use std::rc::Rc;
use taffy::Overflow;

type Binding = Box<dyn FnOnce(&Queue, &mut ElementSpec, &Rc<Cell<Option<ElementId>>>)>;

pub(crate) enum Patch {
    Build(Box<dyn FnOnce(&mut BuildCx<'_>)>),
    Style(ElementId, Declaration),
    Variant(ElementId, usize, usize, Declaration),
    Text(ElementId, Dirty, Box<dyn FnOnce(&mut TextData)>),
    Content(ElementId, String),
    RefreshText(ElementId),
    Cursor(ElementId, Option<Cursor>),
    Enabled(ElementId, bool),
    LayerProp(ElementId, LayerProp, MotionProp, bool),
    Effect(ElementId, EffectSlot, EffectInstance),
    ScrollLink(ElementId, u64, ScrollLinked),
    ScrollTo(ElementId, Point),
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
            Patch::Style(id, declaration) => cx.tree.update_style(id, declaration),
            Patch::Variant(id, slot, index, declaration) => {
                cx.tree.update_variant(id, slot, index, declaration);
            }
            Patch::Text(id, dirty, set) => cx.tree.update_text(id, dirty, set),
            Patch::Content(id, content) => cx.tree.set_text_content(id, content),
            Patch::RefreshText(id) => cx.tree.refresh_text_subtree(id),
            Patch::Cursor(id, cursor) => cx.tree.set_cursor(id, cursor),
            Patch::Enabled(id, enabled) => cx.tree.set_disabled(id, !enabled),
            Patch::LayerProp(id, prop, value, animate) => {
                cx.tree.set_layer_prop(id, prop, value, animate);
            }
            Patch::Effect(id, slot, effect) => cx.tree.set_layer_effect(id, slot, effect),
            Patch::ScrollLink(id, key, link) => cx.tree.set_scroll_link(id, key, link),
            Patch::ScrollTo(id, offset) => cx.tree.scroll_to(id, offset),
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
pub struct Interactions {
    pub hovered: bool,
    pub pressed: bool,
    pub focus: Option<FocusKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusKind {
    Pointer,
    Keyboard,
}

impl Interactions {
    pub fn focused(self) -> bool {
        self.focus.is_some()
    }

    pub fn focus_visible(self) -> bool {
        self.focus == Some(FocusKind::Keyboard)
    }
}

#[derive(Default)]
pub struct Div {
    pub(crate) spec: ElementSpec,
    bindings: Vec<Binding>,
    children: Vec<AnyView>,
    effects: usize,
    autofocus: bool,
}

#[component]
pub fn div() -> Div {
    Div::default()
}

macro_rules! variants {
    ($($name:ident: $which:ident;)*) => {$(
        pub fn $name(self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
            self.variant(Interaction::$which as usize, f(StyleDelta::default()))
        }
    )*};
}

macro_rules! listeners {
    ($($name:ident: $on:ident($event:ident: $arg:ty);)*) => {$(
        #[track_caller]
        pub fn $name(mut self, f: impl Fn($arg) + 'static) -> Self {
            let owner = Scope::current();
            let listener = move |event: &Event<'_>| {
                if let Event::$event(event) = *event {
                    run_in(owner, || f(event));
                }
            };
            self.spec.extras().listeners.push((On::$on, Rc::new(listener)));
            self
        }
    )*};
}

macro_rules! layer_props {
    ($($name:ident: $prop:ident;)*) => {$(
        pub fn $name<T>(self, value: impl IntoProp<MotionProp, T>) -> Self {
            self.layer_prop(LayerProp::$prop, value)
        }
    )*};
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

    pub fn group_hover(
        mut self,
        name: impl Into<String>,
        f: impl FnOnce(StyleDelta) -> StyleDelta,
    ) -> Self {
        let groups = &mut self.spec.extras().groups;
        groups.push((name.into(), Vec::new()));
        let slot = STATES + groups.len() - 1;
        self.variant(slot, f(StyleDelta::default()))
    }

    fn variant(mut self, slot: usize, delta: StyleDelta) -> Self {
        let mut declarations = Vec::with_capacity(delta.0.len());
        for (index, declaration) in delta.0.into_iter().enumerate() {
            let compute = match declaration {
                Prop::Static(declaration) => {
                    declarations.push(declaration);
                    continue;
                }
                Prop::Dynamic(compute) => compute,
            };
            declarations.push(untrack(&compute));
            self = self.bind_prop(
                Prop::Dynamic(compute),
                move |spec, declaration| {
                    let variant = spec.extras().variant_mut(slot);
                    if let Some(entry) = variant.and_then(|variant| variant.get_mut(index)) {
                        *entry = declaration;
                    }
                },
                move |id, declaration| Some(Patch::Variant(id, slot, index, declaration)),
            );
        }
        if let Some(variant) = self.spec.extras().variant_mut(slot) {
            *variant = declarations;
        }
        self
    }

    pub fn test_id(mut self, name: impl Into<String>) -> Self {
        self.spec.extras().test_id = Some(name.into());
        self
    }

    fn bind<T: 'static, U>(
        self,
        value: impl IntoProp<T, U>,
        init: impl FnOnce(&mut ElementSpec, T) + 'static,
        patch: impl Fn(ElementId, T) -> Patch + 'static,
    ) -> Self {
        self.bind_prop(value.into_prop(), init, move |id, value| {
            Some(patch(id, value))
        })
    }

    fn bind_prop<T: 'static>(
        mut self,
        value: Prop<T>,
        init: impl FnOnce(&mut ElementSpec, T) + 'static,
        patch: impl Fn(ElementId, T) -> Option<Patch> + 'static,
    ) -> Self {
        match value {
            Prop::Static(value) => init(&mut self.spec, value),
            Prop::Dynamic(compute) => {
                self.bindings.push(Box::new(move |queue, spec, element| {
                    let (queue, element) = (queue.clone(), element.clone());
                    let initial = Rc::new(Cell::new(None));
                    effect({
                        let initial = initial.clone();
                        move || {
                            let value = compute();
                            match element.get() {
                                Some(id) => {
                                    if let Some(patch) = patch(id, value) {
                                        queue.push(patch);
                                    }
                                }
                                None => initial.set(Some(value)),
                            }
                        }
                    });
                    if let Some(value) = initial.take() {
                        init(spec, value);
                    }
                }));
            }
        }
        self
    }

    fn layer_prop<T>(self, prop: LayerProp, value: impl IntoProp<MotionProp, T>) -> Self {
        let last = Rc::new(Cell::new(None));
        let first = last.clone();
        self.layer().bind_prop(
            value.into_prop(),
            move |spec, value| {
                first.set(Some(value));
                let layer = spec.layer.get_or_insert_default();
                layer.props.push((prop, value));
            },
            move |id, value| {
                let previous = last.replace(Some(value));
                (previous != Some(value)).then_some(Patch::LayerProp(
                    id,
                    prop,
                    value,
                    previous.is_some(),
                ))
            },
        )
    }

    fn apply_effect<T: 'static, U>(self, slot: EffectSlot, effect: impl IntoProp<T, U>) -> Self
    where
        AnyEffect: PropFrom<T>,
    {
        self.layer().bind(
            effect,
            move |spec, effect| {
                let layer = spec.layer.get_or_insert_default();
                layer.effects.push((slot, AnyEffect::prop_from(effect).0));
            },
            move |id, effect| Patch::Effect(id, slot, AnyEffect::prop_from(effect).0),
        )
    }

    pub fn scroll_linked<T>(
        self,
        state: Signal<ScrollState>,
        link: impl IntoProp<ScrollLinked, T>,
    ) -> Self {
        let key = scroll_key(state);
        self.layer().bind_prop(
            link.into_prop(),
            move |spec, link| spec.layer.get_or_insert_default().link = Some((key, link)),
            move |id, link| Some(Patch::ScrollLink(id, key, link)),
        )
    }

    pub(crate) fn scrollable(mut self) -> Self {
        self.spec.extras().scroll.get_or_insert_default();
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

    variants! {
        hover: Hover;
        active: Active;
        focus: Focus;
        focus_visible: FocusVisible;
        disabled: Disabled;
    }

    listeners! {
        on_click: Click(Pointer: &PointerEvent);
        on_pointer_down: PointerDown(Pointer: &PointerEvent);
        on_pointer_up: PointerUp(Pointer: &PointerEvent);
        on_pointer_move: PointerMove(Pointer: &PointerEvent);
        on_key_down: KeyDown(Key: &KeyEvent);
        on_key_up: KeyUp(Key: &KeyEvent);
        on_focus: Focus(Focus: &FocusEvent);
        on_blur: Blur(Focus: &FocusEvent);
        on_hover: Hover(Hover: bool);
        on_ime: Ime(Ime: &ImeEvent);
        on_drag: Drag(Drag: &DragEvent);
        on_scroll: Scroll(Scroll: &ScrollEvent);
        on_file_drop: FileDrop(FileDrop: &FileDropEvent);
    }

    layer_props! {
        translate_x: TranslateX;
        translate_y: TranslateY;
        scale: Scale;
        rotate: Rotate;
        opacity: Opacity;
    }

    pub fn on_press(self, f: impl Fn() + 'static) -> Self {
        let clicked = Rc::new(f);
        let pressed = clicked.clone();
        self.focusable()
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
    }

    pub fn track_interaction(self, state: Signal<Interactions>) -> Self {
        let change = move |f: fn(&mut Interactions, bool), value: bool| {
            let mut next = state.get_untracked();
            f(&mut next, value);
            state.set_if_changed(next);
        };
        self.on_hover(move |inside| {
            change(
                |state, inside| {
                    state.hovered = inside;
                    state.pressed &= inside;
                },
                inside,
            );
        })
        .on_pointer_down(move |_| change(|state, pressed| state.pressed = pressed, true))
        .on_pointer_up(move |_| change(|state, pressed| state.pressed = pressed, false))
        .on_focus(move |event| {
            change(
                |state, visible| {
                    state.focus = Some(if visible {
                        FocusKind::Keyboard
                    } else {
                        FocusKind::Pointer
                    });
                },
                event.visible,
            );
        })
        .on_blur(move |_| {
            change(|state, _| state.focus = None, false);
        })
    }

    pub fn enabled<U>(self, enabled: impl IntoProp<bool, U>) -> Self {
        self.bind(
            enabled,
            |spec, enabled| spec.disabled = !enabled,
            Patch::Enabled,
        )
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
        self.bind(cursor, |spec, cursor| spec.cursor = cursor, Patch::Cursor)
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
        self.bind_prop(
            Prop::Dynamic(Box::new(move || state.get().offset)),
            |_, _| {},
            |id, offset| Some(Patch::ScrollTo(id, offset)),
        )
    }
}

impl Styled for Div {
    fn declare(self, declaration: Prop<Declaration>) -> Self {
        self.bind_prop(
            declaration,
            |spec, declaration| spec.declare(&declaration),
            |id, declaration| Some(Patch::Style(id, declaration)),
        )
    }
}

impl View for Div {
    fn build(self, cx: &mut BuildCx<'_>) {
        self.mount(cx);
    }
}

impl Div {
    pub(crate) fn mount(mut self, cx: &mut BuildCx<'_>) -> ElementId {
        if let Some(text) = self.spec.kind.text_mut() {
            let theme = Theme::current();
            text.theme = Some(theme);
            self = self.bind_prop(
                Prop::Dynamic(Box::new(move || {
                    theme.with(|palette| palette.text);
                })),
                |_, ()| {},
                |id, ()| Some(Patch::RefreshText(id)),
            );
        }
        let extras = self.spec.extras.as_deref();
        if extras.is_some_and(|extras| extras.scroll.is_some()) {
            self.spec.declare(&Declaration::Overflow(Overflow::Scroll));
        }
        let element = Rc::new(Cell::new(None));
        for bind in self.bindings {
            bind(cx.queue, &mut self.spec, &element);
        }
        let id = cx.tree.create_element(self.spec);
        element.set(Some(id));
        if self.autofocus {
            cx.queue.push(Patch::Autofocus(id));
        }
        cx.within(Node::Element(id), |cx| {
            for child in self.children {
                child.build(cx);
            }
        });
        cx.tree.append(cx.parent, id);
        id
    }
}

pub struct Text(pub(crate) Div);

#[component]
pub fn text<U>(children: impl IntoProp<String, U>) -> Text {
    let mut div = div();
    div.spec.kind = Kind::Text(Box::default());
    Text(div.bind(
        children,
        |spec, content| {
            if let Some(text) = spec.kind.text_mut() {
                text.content = content;
            }
        },
        Patch::Content,
    ))
}

#[component]
pub fn rich_text<T, U>(
    children: impl IntoProp<String, T>,
    spans: impl IntoProp<Vec<(Range<usize>, SpanStyle)>, U>,
) -> Text {
    text(children).spans(spans)
}

impl Text {
    pub fn spans<U>(self, spans: impl IntoProp<Vec<(Range<usize>, SpanStyle)>, U>) -> Self {
        self.text_prop(spans, Dirty::Layout, |text, spans| text.style.spans = spans)
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
        dirty: Dirty,
        set: fn(&mut TextData, T),
    ) -> Self {
        Text(self.0.bind(
            value,
            move |spec, value| {
                if let Some(text) = spec.kind.text_mut() {
                    set(text, value);
                }
            },
            move |id, value| Patch::Text(id, dirty, Box::new(move |text| set(text, value))),
        ))
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

    pub fn color<U>(self, color: impl IntoProp<Color, U>) -> Self {
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

styled!(Text => 0);

impl View for Text {
    fn build(self, cx: &mut BuildCx<'_>) {
        self.0.mount(cx);
    }
}
