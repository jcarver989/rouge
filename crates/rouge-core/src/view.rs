use crate::event::{
    Chord, Cursor, DragEvent, FocusEvent, ImeEvent, KeyEvent, PointerEvent, ScrollEvent,
};
use crate::focus::FocusHandle;
use crate::motion::MotionProp;
use crate::prop::{IntoEffect, IntoProp, Prop};
use crate::scroll::Scrollable;
use crate::style::{Dirty, Style, Styled};
use crate::styled;
use crate::tree::{ElementId, ElementSpec, Event, Interaction, On, TextData, Tree};
use rouge_reactive::{Signal, effect};
use rouge_render::{
    BlendMode, Color, Effect, EffectInstance, EffectSlot, Keyframes, LayerCommand, LayerProp,
    LayoutTransition, Mask, Rect, ScrollState, Timing,
};
use rouge_text::{TextAlign, TextStyle, TextSystem};
use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use taffy::{Overflow, Point as Axes};
use tokio::sync::mpsc::UnboundedSender;

pub(crate) type Patch = Box<dyn FnOnce(&mut BuildCx<'_>)>;
pub(crate) type Patches = UnboundedSender<Patch>;
type Binding = Box<dyn FnOnce(&Patches, ElementId)>;

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

#[derive(Default)]
pub struct Div {
    pub(crate) spec: ElementSpec,
    bindings: Vec<Binding>,
    children: Vec<AnyView>,
    effects: usize,
    autofocus: bool,
}

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
        pub fn $name(mut self, f: impl Fn($arg) + 'static) -> Self {
            let listener = move |event: &Event<'_>| {
                if let Event::$event(event) = *event {
                    f(event);
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
    fn bind<T: 'static, U>(
        self,
        value: impl IntoProp<T, U>,
        init: impl FnOnce(&mut ElementSpec, T),
        apply: impl Fn(&mut Tree, ElementId, T) + 'static,
    ) -> Self {
        self.bind_prop(value.into_prop(), init, apply)
    }

    fn bind_prop<T: 'static>(
        mut self,
        value: Prop<T>,
        init: impl FnOnce(&mut ElementSpec, T),
        apply: impl Fn(&mut Tree, ElementId, T) + 'static,
    ) -> Self {
        match value {
            Prop::Static(value) => init(&mut self.spec, value),
            Prop::Dynamic(compute) => self.bindings.push(Box::new(move |patches, id| {
                let (patches, apply) = (patches.clone(), Rc::new(apply));
                effect(move || {
                    let (value, apply) = (compute(), apply.clone());
                    send(&patches, move |cx| apply(cx.tree, id, value));
                });
            })),
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
    }

    layer_props! {
        translate_x: TranslateX;
        translate_y: TranslateY;
        scale: Scale;
        rotate: Rotate;
        opacity: Opacity;
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

    pub fn cursor(mut self, cursor: Cursor) -> Self {
        self.spec.cursor = Some(cursor);
        self
    }

    pub fn track_bounds(mut self, bounds: Signal<Rect>) -> Self {
        self.spec.bounds_observer = Some(bounds);
        self
    }

    pub fn bind_key<T: Any>(mut self, chord: Chord, action: T) -> Self {
        self.spec.bindings.push((chord, Rc::new(action)));
        self
    }

    pub fn on_action<T: Any>(mut self, f: impl Fn(&T) + 'static) -> Self {
        let handler = move |action: &dyn Any| {
            if let Some(action) = action.downcast_ref::<T>() {
                f(action);
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
        self.bindings.push(Box::new(move |patches, id| {
            let patches = patches.clone();
            effect(move || {
                let offset = state.get().offset;
                send(&patches, move |cx| cx.tree.scroll_to(id, offset));
            });
        }));
        self
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
        if self.spec.scroll.is_some() {
            self.spec.style.layout.overflow = Axes {
                x: Overflow::Scroll,
                y: Overflow::Scroll,
            };
        }
        let id = cx.tree.create_element(self.spec);
        for bind in self.bindings {
            bind(cx.patches, id);
        }
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

pub fn text<U>(content: impl IntoProp<String, U>) -> Text {
    Text(div().bind(
        content,
        |spec, content| spec.text.get_or_insert_default().content = content,
        Tree::set_text_content,
    ))
}

impl Text {
    fn text_prop<T: 'static, U>(
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

    pub fn text_size<U>(self, size: impl IntoProp<f32, U>) -> Self {
        self.text_prop(size, Dirty::Layout, |text, size| text.style.size = size)
    }

    pub fn color<U>(self, color: impl IntoProp<Color, U>) -> Self {
        self.text_prop(color, Dirty::Paint, |text, color| text.color = color)
    }

    pub fn weight<U>(self, weight: impl IntoProp<f32, U>) -> Self {
        self.text_prop(weight, Dirty::Layout, |text, weight| {
            text.style.weight = weight;
        })
    }

    pub fn bold(self) -> Self {
        self.weight(700.0)
    }

    pub fn italic(self) -> Self {
        self.text_style(|style| style.italic = true)
    }

    pub fn font(self, family: impl Into<Cow<'static, str>>) -> Self {
        self.text_style(|style| style.family = family.into())
    }

    pub fn line_height(self, multiple: f32) -> Self {
        self.text_style(|style| style.line_height = multiple)
    }

    pub fn align(self, align: TextAlign) -> Self {
        self.text_style(|style| style.align = align)
    }

    pub fn no_wrap(self) -> Self {
        self.text_style(|style| style.wrap = false)
    }
}

styled!(Text => 0);

impl View for Text {
    fn build(mut self, cx: &mut BuildCx<'_>) -> ElementId {
        self.0.spec.text.get_or_insert_default();
        self.0.build(cx)
    }
}
