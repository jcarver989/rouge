use crate::Cursor;
use crate::length::Length;
use rouge_reactive::{ReadSignal, Signal, memo};
use rouge_render::{Color, Corners, EffectInstance, ShaderEffect};
use std::borrow::Cow;

pub enum Prop<T> {
    Static(T),
    Dynamic(Box<dyn Fn() -> T>),
}

impl<T: Clone> Prop<T> {
    pub fn get(&self) -> T {
        match self {
            Prop::Static(value) => value.clone(),
            Prop::Dynamic(compute) => compute(),
        }
    }
}

impl<T: 'static> Prop<T> {
    pub fn map<U>(self, f: impl Fn(T) -> U + 'static) -> Prop<U> {
        match self {
            Prop::Static(value) => Prop::Static(f(value)),
            Prop::Dynamic(compute) => Prop::Dynamic(Box::new(move || f(compute()))),
        }
    }
}

impl<T: Clone + PartialEq + 'static> Prop<T> {
    pub fn memo(self) -> Prop<T> {
        match self {
            Prop::Static(value) => Prop::Static(value),
            Prop::Dynamic(compute) => {
                let shared = memo(compute);
                Prop::Dynamic(Box::new(move || shared.get()))
            }
        }
    }
}

impl<T: Default> Default for Prop<T> {
    fn default() -> Self {
        Prop::Static(T::default())
    }
}

pub trait PropFrom<T> {
    fn prop_from(value: T) -> Self;
}

impl<T> PropFrom<T> for T {
    fn prop_from(value: T) -> Self {
        value
    }
}

impl PropFrom<f32> for Length {
    fn prop_from(value: f32) -> Self {
        Length::Px(value)
    }
}

impl PropFrom<i32> for Length {
    fn prop_from(value: i32) -> Self {
        Length::Px(value as f32)
    }
}

impl PropFrom<Cursor> for Option<Cursor> {
    fn prop_from(value: Cursor) -> Self {
        Some(value)
    }
}

impl PropFrom<&str> for String {
    fn prop_from(value: &str) -> Self {
        value.to_owned()
    }
}

impl PropFrom<&'static str> for Cow<'static, str> {
    fn prop_from(value: &'static str) -> Self {
        Cow::Borrowed(value)
    }
}

impl PropFrom<String> for Cow<'static, str> {
    fn prop_from(value: String) -> Self {
        Cow::Owned(value)
    }
}

impl PropFrom<u32> for Color {
    fn prop_from(value: u32) -> Self {
        Color::rgb(value)
    }
}

impl PropFrom<f32> for Corners {
    fn prop_from(value: f32) -> Self {
        Corners::all(value)
    }
}

impl PropFrom<Length> for Corners {
    fn prop_from(value: Length) -> Self {
        match value {
            Length::Px(value) => Corners::all(value),
            Length::Fraction(_) | Length::Auto => Corners::ZERO,
        }
    }
}

pub struct AnyEffect(pub(crate) EffectInstance);

impl<T: ShaderEffect> PropFrom<T> for AnyEffect {
    fn prop_from(effect: T) -> Self {
        AnyEffect(EffectInstance::new(&effect))
    }
}

pub trait IntoProp<T, U> {
    fn into_prop(self) -> Prop<T>;
}

#[doc(hidden)]
pub(crate) mod markers {
    pub struct Value;
    pub struct Func;
    pub struct Reactive;
    pub struct Same;
}

impl<T: PropFrom<U>, U> IntoProp<T, markers::Value> for U {
    fn into_prop(self) -> Prop<T> {
        Prop::Static(T::prop_from(self))
    }
}

impl<T: PropFrom<U>, U, V: Fn() -> U + 'static> IntoProp<T, markers::Func> for V {
    fn into_prop(self) -> Prop<T> {
        Prop::Dynamic(Box::new(move || T::prop_from(self())))
    }
}

impl<T: PropFrom<U>, U: Clone + 'static> IntoProp<T, markers::Reactive> for ReadSignal<U> {
    fn into_prop(self) -> Prop<T> {
        Prop::Dynamic(Box::new(move || T::prop_from(self.get())))
    }
}

impl<T: PropFrom<U>, U: Clone + 'static> IntoProp<T, markers::Reactive> for Signal<U> {
    fn into_prop(self) -> Prop<T> {
        self.read_only().into_prop()
    }
}

impl<T> IntoProp<T, markers::Same> for Prop<T> {
    fn into_prop(self) -> Prop<T> {
        self
    }
}
