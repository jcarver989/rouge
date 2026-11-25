use crate::{Fonts, Materials, Radii};
use rouge_reactive::{provide_context, use_context};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiStyle {
    pub fonts: Fonts,
    pub radii: Radii,
    pub materials: Materials,
}

impl UiStyle {
    pub fn provide(self) {
        provide_context(self);
    }

    pub fn current() -> Self {
        use_context().unwrap_or_default()
    }
}
