use rouge_render::Color;

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

theme! {
    pub struct Theme from Palette {
    background: Color,
    surface: Color,
    text: Color,
    text_muted: Color,
    accent: Color,
    accent_text: Color,
    border: Color,
    hover: Color,
    pressed: Color,
    focus_ring: Color,
    danger: Color,
    scrim: Color,
    radius: f32,
    text_size: f32,
    spacing: f32,
    }
}

impl Palette {
    pub fn light() -> Palette {
        Palette {
            background: Color::rgb(0xf5f5f7),
            surface: Color::WHITE,
            text: Color::rgb(0x1d1d1f),
            text_muted: Color::rgb(0x6e6e73),
            accent: Color::rgb(0x0a6cff),
            accent_text: Color::WHITE,
            border: Color::rgb(0xd2d2d7),
            hover: Color::new(0.0, 0.0, 0.0, 0.05),
            pressed: Color::new(0.0, 0.0, 0.0, 0.1),
            focus_ring: Color::new(0.04, 0.42, 1.0, 0.55),
            danger: Color::rgb(0xd70015),
            scrim: Color::new(0.0, 0.0, 0.0, 0.35),
            radius: 8.0,
            text_size: 14.0,
            spacing: 8.0,
        }
    }

    pub fn dark() -> Palette {
        Palette {
            background: Color::rgb(0x1c1c1e),
            surface: Color::rgb(0x2c2c2e),
            text: Color::rgb(0xf5f5f7),
            text_muted: Color::rgb(0x98989d),
            accent: Color::rgb(0x0a84ff),
            accent_text: Color::WHITE,
            border: Color::rgb(0x48484a),
            hover: Color::new(1.0, 1.0, 1.0, 0.07),
            pressed: Color::new(1.0, 1.0, 1.0, 0.13),
            focus_ring: Color::new(0.04, 0.52, 1.0, 0.65),
            danger: Color::rgb(0xff453a),
            scrim: Color::new(0.0, 0.0, 0.0, 0.5),
            ..Palette::light()
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Palette::light()
    }
}

pub fn provide_theme(palette: Palette) -> Theme {
    Theme::provide(palette)
}

#[track_caller]
pub fn use_theme() -> Theme {
    Theme::current()
}
