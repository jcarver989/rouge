use crate::length::Length;
use rouge_reactive::{ReadSignal, Signal};
use rouge_render::{Color, Corners, Effect};

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

impl PropFrom<&str> for String {
    fn prop_from(value: &str) -> Self {
        value.to_owned()
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

pub trait IntoProp<T, U> {
    fn into_prop(self) -> Prop<T>;
}

#[doc(hidden)]
pub(crate) mod markers {
    pub struct Value;
    pub struct Func;
    pub struct Reactive;
    pub struct Indexed;
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

pub trait IntoEffect<T: Effect, U> {
    fn into_effect(self) -> Prop<T>;
}

impl<T: Effect> IntoEffect<T, markers::Value> for T {
    fn into_effect(self) -> Prop<T> {
        Prop::Static(self)
    }
}

impl<T: Effect, U: Fn() -> T + 'static> IntoEffect<T, markers::Func> for U {
    fn into_effect(self) -> Prop<T> {
        Prop::Dynamic(Box::new(self))
    }
}

impl<T: Effect + Clone> IntoEffect<T, markers::Reactive> for ReadSignal<T> {
    fn into_effect(self) -> Prop<T> {
        Prop::Dynamic(Box::new(move || self.get()))
    }
}

impl<T: Effect + Clone> IntoEffect<T, markers::Reactive> for Signal<T> {
    fn into_effect(self) -> Prop<T> {
        self.read_only().into_effect()
    }
}
