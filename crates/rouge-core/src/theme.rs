use rouge_reactive::{Signal, batch, expect_context, provide_context, signal};
use rouge_render::Color;
use rouge_theme::{
    ButtonVariant, ControlState, NoticeTone, SelectionState, SemanticStyle, UiPalette,
};
use std::ops::Deref;

#[macro_export]
macro_rules! theme {
    ($vis:vis struct $theme:ident from $palette:ident { $($name:ident: $ty:ty,)* }) => {
        #[derive(Clone, Copy, Debug, PartialEq)]
        $vis struct $palette {
            $(pub $name: $ty,)*
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis struct $theme {
            $(pub $name: $crate::__private::Signal<$ty>,)*
        }

        impl $theme {
            pub fn new(palette: $palette) -> $theme {
                $theme { $($name: $crate::__private::signal(palette.$name),)* }
            }

            pub fn provide(palette: $palette) -> $theme {
                let theme = $theme::new(palette);
                $crate::__private::provide_context(theme);
                theme
            }

            #[track_caller]
            pub fn current() -> $theme {
                $crate::__private::expect_context::<$theme>()
            }

            pub fn set(&self, palette: $palette) {
                $crate::__private::batch(|| {
                    $(self.$name.set_if_changed(palette.$name);)*
                });
            }

            pub fn get(&self) -> $palette {
                $palette { $($name: self.$name.get(),)* }
            }

            pub fn palette(&self) -> $palette {
                $palette { $($name: self.$name.get_untracked(),)* }
            }
        }
    };
}

pub type Palette = UiPalette<Color>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme(UiPalette<Signal<Color>>);

impl Theme {
    pub fn new<T: Into<Color>>(palette: UiPalette<T>) -> Theme {
        Theme(palette.map(|color| signal(color.into())))
    }

    pub fn provide<T: Into<Color>>(palette: UiPalette<T>) -> Theme {
        let theme = Theme::new(palette);
        provide_context(theme);
        theme
    }

    #[track_caller]
    pub fn current() -> Theme {
        expect_context()
    }

    pub fn set<T: Into<Color>>(&self, palette: UiPalette<T>) {
        batch(|| {
            self.0.zip(palette).for_each(|(role, color)| {
                role.set_if_changed(color.into());
            });
        });
    }

    pub fn get(&self) -> Palette {
        self.0.map(|role| role.get())
    }

    pub fn palette(&self) -> Palette {
        self.0.map(|role| role.get_untracked())
    }

    pub fn control_style(
        &self,
        variant: ButtonVariant,
        state: ControlState,
    ) -> SemanticStyle<Color> {
        self.get().control_style(variant, state)
    }

    pub fn selection_style(&self, state: SelectionState) -> SemanticStyle<Color> {
        self.get().selection_style(state)
    }

    pub fn notice_style(&self, tone: NoticeTone) -> SemanticStyle<Color> {
        self.get().notice_style(tone)
    }
}

impl Deref for Theme {
    type Target = UiPalette<Signal<Color>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
