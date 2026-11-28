use crate::engine::Placement;
use rouge_core::{Rect, occlude, window_scope};
use rouge_reactive::{
    Memo, Scope, Signal, memo, on_cleanup, provide_context, signal, use_context, watch,
};

type Request = Memo<(Placement, Rect)>;

struct Mount {
    page: u64,
    exclusive: bool,
    request: Request,
}

#[derive(Clone, Copy)]
pub(crate) struct Mounts(Signal<Vec<Mount>>);

impl Mounts {
    fn current() -> Self {
        use_context().unwrap_or_else(|| {
            let mounts = Self(signal(Vec::new()));
            provide_context(mounts);
            mounts
        })
    }

    pub(crate) fn attach(
        self,
        page: u64,
        exclusive: bool,
        request: impl Fn() -> (Placement, Rect) + 'static,
    ) {
        let request = memo(request);
        self.0.update(|mounts| {
            mounts.push(Mount {
                page,
                exclusive,
                request,
            });
        });
        on_cleanup(move || {
            self.0
                .try_update(|mounts| mounts.retain(|mount| mount.request != request));
        });
    }

    fn winner(self, page: u64) -> Option<Request> {
        self.0.with(|mounts| {
            mounts
                .iter()
                .rev()
                .find(|mount| mount.exclusive)
                .or_else(|| mounts.iter().rev().find(|mount| mount.page == page))
                .filter(|mount| mount.page == page)
                .map(|mount| mount.request)
        })
    }
}

pub(crate) struct Presentation {
    pub(crate) mounts: Mounts,
    scope: Scope,
}

impl Presentation {
    pub(crate) fn new(page: u64, place: impl Fn(Placement) + 'static) -> Self {
        let window = window_scope();
        let mounts = window.run(Mounts::current);
        let scope = window.child();
        scope.run(|| {
            let winner = memo(move || mounts.winner(page));
            let shown = move || {
                winner
                    .get()
                    .and_then(|request| request.try_get())
                    .unwrap_or_default()
            };
            occlude(move || {
                let (_, bounds) = shown();
                bounds
            });
            watch(
                move || {
                    let (placement, _) = shown();
                    placement
                },
                move |current, previous| {
                    if previous.is_some() || current.visible() {
                        place(*current);
                    }
                },
            );
            on_cleanup(move || {
                mounts
                    .0
                    .try_update(|mounts| mounts.retain(|mount| mount.page != page));
            });
        });
        Self { mounts, scope }
    }
}

impl Drop for Presentation {
    fn drop(&mut self) {
        if self.scope.is_alive() {
            self.scope.dispose();
        }
    }
}
