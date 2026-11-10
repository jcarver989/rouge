use crate::event::Chord;
use rouge_reactive::{Memo, Signal, expect_context, memo};
use std::any::{Any, TypeId};

#[derive(Clone, Copy)]
pub(crate) struct Keymap(pub Signal<Vec<(TypeId, Chord)>>);

pub fn use_binding<T: Any>() -> Memo<Option<Chord>> {
    let keymap = expect_context::<Keymap>();
    memo(move || {
        keymap.0.with(|bindings| {
            bindings
                .iter()
                .find(|(kind, _)| *kind == TypeId::of::<T>())
                .map(|(_, chord)| chord.clone())
        })
    })
}
