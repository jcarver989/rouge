use crate::runtime::{NodeId, try_with_rt, with_rt};
#[cfg(debug_assertions)]
use std::panic::Location;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Handle {
    pub(crate) id: NodeId,
    #[cfg(debug_assertions)]
    created_at: &'static Location<'static>,
}

impl Handle {
    #[track_caller]
    pub(crate) fn new(id: NodeId) -> Self {
        Handle {
            id,
            #[cfg(debug_assertions)]
            created_at: Location::caller(),
        }
    }

    pub(crate) fn is_alive(&self) -> bool {
        try_with_rt(|rt| rt.is_alive(self.id)).unwrap_or(false)
    }

    pub(crate) fn dispose(&self) {
        try_with_rt(|rt| rt.dispose(self.id));
    }

    #[cold]
    #[track_caller]
    pub(crate) fn panic_disposed(&self, what: &str, action: &str) -> ! {
        #[cfg(debug_assertions)]
        panic!(
            "rouge-reactive: tried to {action} a {what} that was already disposed \
             (created at {}). Its owning scope no longer exists; create the {what} in \
             a longer-lived scope, or use the `try_` variant.",
            self.created_at
        );
        #[cfg(not(debug_assertions))]
        panic!("rouge-reactive: tried to {action} a {what} that was already disposed");
    }

    pub(crate) fn try_read<T: 'static, U>(
        &self,
        tracked: bool,
        f: impl FnOnce(&T) -> U,
    ) -> Option<U> {
        let value = with_rt(|rt| {
            rt.update_if_necessary(self.id);
            let value = rt.value(self.id)?;
            if tracked {
                rt.track(self.id);
            }
            Some(value)
        })?;
        let guard = value.try_borrow().unwrap_or_else(|_| {
            panic!("rouge-reactive: value read while it is being mutated (read inside `update` of the same signal?)")
        });
        let typed = guard
            .downcast_ref::<Option<T>>()
            .expect("rouge-reactive: handle type does not match stored value")
            .as_ref()
            .expect("rouge-reactive: memo read from inside its own computation");
        Some(f(typed))
    }

    pub(crate) fn try_update<T: 'static, U>(&self, f: impl FnOnce(&mut T) -> U) -> Option<U> {
        let value = with_rt(|rt| rt.value(self.id))?;
        let result = {
            let mut guard = value.try_borrow_mut().unwrap_or_else(|_| {
                panic!("rouge-reactive: signal written while it is being read (write inside `with` of the same signal?)")
            });
            let typed = guard
                .downcast_mut::<Option<T>>()
                .expect("rouge-reactive: handle type does not match stored value")
                .as_mut()
                .expect("rouge-reactive: signals always hold a value");
            f(typed)
        };
        with_rt(|rt| rt.notify(self.id));
        Some(result)
    }
}
