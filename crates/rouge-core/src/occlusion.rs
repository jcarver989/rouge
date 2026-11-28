use rouge_reactive::{Memo, Signal, expect_context, memo, on_cleanup, signal};
use rouge_render::Rect;

#[derive(Clone, Copy)]
pub(crate) struct Occlusions(Signal<Vec<Memo<Rect>>>);

impl Occlusions {
    pub(crate) fn new() -> Self {
        Self(signal(Vec::new()))
    }
}

#[track_caller]
pub fn occlude(region: impl Fn() -> Rect + 'static) {
    let regions = expect_context::<Occlusions>().0;
    let region = memo(region);
    regions.update(|regions| regions.push(region));
    on_cleanup(move || {
        regions.try_update(|regions| regions.retain(|held| *held != region));
    });
}

pub fn occlusions() -> Vec<Rect> {
    expect_context::<Occlusions>().0.with(|regions| {
        regions
            .iter()
            .filter_map(Memo::try_get)
            .filter(|rect| !rect.is_empty())
            .collect()
    })
}
