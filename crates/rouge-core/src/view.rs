use crate::canvas::TextHit;
use crate::event::{
    Cursor, DragEvent, FileDropEvent, FocusEvent, ImeEvent, IntoChord, Key, KeyEvent, Modifiers,
    PointerEvent, ScrollEvent,
};
use crate::focus::FocusHandle;
use crate::length::Length;
use crate::motion::MotionProp;
use crate::prop::{IntoEffect, IntoProp, Prop};
use crate::scroll::{Scrollable, scroll_key};
use crate::style::{Dirty, Style, Styled};
use crate::styled;
use crate::theme::Theme;
use crate::tree::{ElementId, ElementSpec, Event, Interaction, On, TextData, Tree};
use rouge_macros::component;
use rouge_reactive::{Scope, Signal, effect, run_in};
use rouge_render::{
    BlendMode, Color, Effect, EffectInstance, EffectSlot, Keyframes, LayerCommand, LayerProp,
    LayoutTransition, Mask, Rect, ScrollLinked, ScrollState, Size, Timing,
};
use rouge_text::{SpanStyle, TextAlign, TextStyle, TextSystem, WrapIndent};
use std::any::{Any, TypeId};
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use taffy::{Overflow, Point as Axes};
use tokio::sync::mpsc::UnboundedSender;

pub(crate) type Patch = Box<dyn FnOnce(&mut BuildCx<'_>)>;
pub(crate) type Patches = UnboundedSender<Patch>;
type Binding = Box<dyn FnOnce(&Patches, &mut ElementSpec, &Rc<Cell<Option<ElementId>>>)>;

pub(crate) fn send(patches: &Patches, patch: impl FnOnce(&mut BuildCx<'_>) + 'static) {
    let _ = patches.send(Box::new(patch));
}

pub struct BuildCx<'a> {
    pub(crate) tree: &'a mut Tree,
    pub(crate) patches: &'a Patches,
    pub(crate) text: &'a mut TextSystem,
}

pub trait View: 'static {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId;

    fn into_any(self) -> AnyView
    where
        Self: Sized,
    {
        AnyView(Box::new(move |cx| self.build(cx)))
    }
}

pub struct AnyView(Box<dyn FnOnce(&mut BuildCx<'_>) -> ElementId>);

impl View for AnyView {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        (self.0)(cx)
    }

    fn into_any(self) -> AnyView {
        self
    }
}

impl<T: View> View for Vec<T> {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let anchor = cx.tree.create_anchor();
        let followers = self.into_iter().map(|view| view.build(cx)).collect();
        cx.tree.set_followers(anchor, followers);
        anchor
    }
}

impl View for () {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        cx.tree.create_anchor()
    }
}

impl<T: View> View for Option<T> {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        match self {
            Some(view) => view.build(cx),
            None => cx.tree.create_anchor(),
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
        pub fn $name(mut self, f: impl Fn(Style) -> Style + 'static) -> Self {
            self.spec.variants[Interaction::$which as usize] = Some(Rc::new(f));
            self
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
            self.spec.listeners[On::$on as usize].push(Rc::new(listener));
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
        self.spec.preserve_focus = true;
        self
    }

    pub fn root_host(mut self) -> Self {
        self.spec.root_host = true;
        self
    }

    pub fn group(mut self, name: impl Into<String>) -> Self {
        self.spec.group = Some(name.into());
        self
    }

    pub fn test_id(mut self, name: impl Into<String>) -> Self {
        self.spec.test_id = Some(name.into());
        self
    }

    fn bind<T: 'static, U>(
        self,
        value: impl IntoProp<T, U>,
        init: impl FnOnce(&mut ElementSpec, T) + 'static,
        apply: impl Fn(&mut Tree, ElementId, T) + 'static,
    ) -> Self {
        self.bind_prop(value.into_prop(), init, apply)
    }

    fn bind_prop<T: 'static>(
        mut self,
        value: Prop<T>,
        init: impl FnOnce(&mut ElementSpec, T) + 'static,
        apply: impl Fn(&mut Tree, ElementId, T) + 'static,
    ) -> Self {
        match value {
            Prop::Static(value) => init(&mut self.spec, value),
            Prop::Dynamic(compute) => {
                self.bindings.push(Box::new(move |patches, spec, element| {
                    let (patches, apply, element) =
                        (patches.clone(), Rc::new(apply), element.clone());
                    let initial = Rc::new(Cell::new(None));
                    effect({
                        let initial = initial.clone();
                        move || {
                            let value = compute();
                            match element.get() {
                                Some(id) => {
                                    let apply = apply.clone();
                                    send(&patches, move |cx| apply(cx.tree, id, value));
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

    fn command(mut self, command: LayerCommand) -> Self {
        self.spec
            .layer
            .get_or_insert_default()
            .commands
            .push(command);
        self
    }

    fn layer_bind<T: 'static>(
        self,
        value: Prop<T>,
        command: impl Fn(T, bool) -> Option<LayerCommand> + 'static,
    ) -> Self {
        let command = Rc::new(command);
        let first = command.clone();
        self.layer().bind_prop(
            value,
            move |spec, value| {
                let commands = &mut spec.layer.get_or_insert_default().commands;
                commands.extend(first(value, false));
            },
            move |tree, id, value| {
                if let Some(command) = command(value, true) {
                    tree.layer_command(id, command);
                }
            },
        )
    }

    fn layer_prop<T>(self, prop: LayerProp, value: impl IntoProp<MotionProp, T>) -> Self {
        let last = Cell::new(None);
        self.layer_bind(value.into_prop(), move |value, dynamic| {
            let previous = last.replace(Some(value));
            (previous != Some(value)).then_some(match value {
                MotionProp::Motion(motion) => LayerCommand::Bind {
                    prop,
                    motion: motion.id(),
                },
                MotionProp::Value(value) => LayerCommand::Set {
                    prop,
                    value,
                    animate: dynamic && previous.is_some(),
                },
            })
        })
    }

    fn apply_effect<T: Effect, U>(self, slot: EffectSlot, effect: impl IntoEffect<T, U>) -> Self {
        self.layer_bind(effect.into_effect(), move |effect, animate| {
            Some(LayerCommand::Effect {
                slot,
                effect: Some(EffectInstance::new(&effect)),
                animate,
            })
        })
    }

    pub fn scroll_linked<T>(
        self,
        state: Signal<ScrollState>,
        link: impl IntoProp<ScrollLinked, T>,
    ) -> Self {
        let key = scroll_key(state);
        self.layer_bind(link.into_prop(), move |link, _| {
            Some(LayerCommand::ScrollLink(Some((key, link))))
        })
    }

    pub(crate) fn scrollable(mut self) -> Self {
        self.spec.scroll.get_or_insert_default();
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
            |tree, id, enabled| tree.set_disabled(id, !enabled),
        )
    }

    pub fn focusable(mut self) -> Self {
        self.spec.focus.get_or_insert_default();
        self
    }

    pub fn tab_index(mut self, index: i32) -> Self {
        self.spec.focus.get_or_insert_default().tab_index = index;
        self
    }

    pub fn track_focus(mut self, handle: FocusHandle) -> Self {
        self.spec.focus.get_or_insert_default().handle = Some(handle.binding());
        self
    }

    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self.focusable()
    }

    pub fn focus_scope(mut self) -> Self {
        self.spec.focus_scope = true;
        self
    }

    pub fn pass_through(mut self) -> Self {
        self.spec.pass_through = true;
        self
    }

    pub fn cursor<U>(self, cursor: impl IntoProp<Option<Cursor>, U>) -> Self {
        self.bind(
            cursor,
            |spec, cursor| spec.cursor = cursor,
            Tree::set_cursor,
        )
    }

    pub fn track_size(mut self, size: Signal<Size>) -> Self {
        self.spec.size_observer = Some(size);
        self
    }

    pub fn track_bounds(mut self, bounds: Signal<Rect>) -> Self {
        self.spec.bounds_observer = Some(bounds);
        self
    }

    pub fn track_paint_region(mut self, region: Signal<crate::PaintRegion>) -> Self {
        self.spec.paint_region = Some(region);
        self
    }

    #[track_caller]
    pub fn bind_key<T: Any>(mut self, chord: impl IntoChord, action: T) -> Self {
        self.spec
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
            .actions
            .push((TypeId::of::<T>(), Rc::new(handler)));
        self
    }

    pub fn layer(mut self) -> Self {
        self.spec.layer.get_or_insert_default();
        self
    }

    pub fn rasterize(self) -> Self {
        self.command(LayerCommand::Rasterize(true))
    }

    pub fn effect<T: Effect, U>(mut self, effect: impl IntoEffect<T, U>) -> Self {
        let slot = EffectSlot::Content(self.effects);
        self.effects += 1;
        self.apply_effect(slot, effect)
    }

    pub fn backdrop<T: Effect, U>(self, effect: impl IntoEffect<T, U>) -> Self {
        self.apply_effect(EffectSlot::Backdrop, effect)
    }

    pub fn mask<U>(self, mask: impl IntoEffect<Mask, U>) -> Self {
        self.apply_effect(EffectSlot::Mask, mask)
    }

    pub fn blend<U>(self, mode: impl IntoEffect<BlendMode, U>) -> Self {
        self.apply_effect(EffectSlot::Blend, mode)
    }

    pub fn transition(self, timing: impl Into<Timing>) -> Self {
        self.command(LayerCommand::Transition(Some(timing.into())))
    }

    pub fn layout_transition(self, transition: impl Into<LayoutTransition>) -> Self {
        self.command(LayerCommand::LayoutTransition(Some(transition.into())))
    }

    pub fn animate(self, keyframes: Keyframes) -> Self {
        self.command(LayerCommand::Animate(keyframes))
    }
}

impl Scrollable for Div {
    fn scroll_state(mut self, state: Signal<ScrollState>) -> Self {
        self.spec.scroll.get_or_insert_default().observer = Some(state);
        self.bind_prop(
            Prop::Dynamic(Box::new(move || state.get().offset)),
            |_, _| {},
            Tree::scroll_to,
        )
    }
}

impl Styled for Div {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.spec.style
    }

    fn prop<T: 'static, U>(
        self,
        value: impl IntoProp<T, U>,
        dirty: Dirty,
        set: fn(&mut Style, T),
    ) -> Self {
        self.bind(
            value,
            move |spec, value| set(&mut spec.style, value),
            move |tree, id, value| tree.update_style(id, dirty, |style| set(style, value)),
        )
    }
}

impl View for Div {
    fn build(mut self, cx: &mut BuildCx<'_>) -> ElementId {
        if let Some(text) = &mut self.spec.text {
            let theme = Theme::current();
            text.theme = Some(theme);
            self = self.bind_prop(
                Prop::Dynamic(Box::new(move || {
                    theme.text.get();
                })),
                |_, ()| {},
                |tree, id, ()| tree.refresh_text_subtree(id),
            );
        }
        if self.spec.scroll.is_some() {
            self.spec.style.layout.overflow = Axes {
                x: Overflow::Scroll,
                y: Overflow::Scroll,
            };
        }
        let element = Rc::new(Cell::new(None));
        for bind in self.bindings {
            bind(cx.patches, &mut self.spec, &element);
        }
        let id = cx.tree.create_element(self.spec);
        element.set(Some(id));
        if self.autofocus {
            send(cx.patches, move |cx| cx.tree.autofocus(id));
        }
        for child in self.children {
            let child = child.build(cx);
            cx.tree.append_child(id, child);
        }
        id
    }
}

pub struct Text(pub(crate) Div);

#[component]
pub fn text<U>(children: impl IntoProp<String, U>) -> Text {
    Text(div().bind(
        children,
        |spec, content| spec.text.get_or_insert_default().content = content,
        Tree::set_text_content,
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
        self.0.spec.text_hover = Some(Rc::new(move |hit: Option<TextHit>, modifiers| {
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
        self.0.spec.text_click = Some(Rc::new(move |hit: Option<TextHit>, modifiers| {
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
            move |spec, value| set(spec.text.get_or_insert_default(), value),
            move |tree, id, value| tree.update_text(id, dirty, |text| set(text, value)),
        ))
    }

    fn text_style(mut self, set: impl FnOnce(&mut TextStyle)) -> Self {
        set(&mut self.0.spec.text.get_or_insert_default().style);
        self
    }

    pub fn text_size<U>(self, size: impl IntoProp<Length, U>) -> Self {
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
    fn build(mut self, cx: &mut BuildCx<'_>) -> ElementId {
        self.0.spec.text.get_or_insert_default();
        self.0.build(cx)
    }
}
