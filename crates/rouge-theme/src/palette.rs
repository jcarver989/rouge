use crate::Rgba;
use serde::{Deserialize, Serialize};

macro_rules! roles {
    ($($role:ident: $name:ident),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub struct UiPalette<T = Rgba> {
            $(pub $role: T,)*
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum Role {
            $($name,)*
        }

        impl<T> UiPalette<T> {
            pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> UiPalette<U> {
                UiPalette { $($role: f(self.$role),)* }
            }

            #[must_use]
            pub const fn get(&self, role: Role) -> &T {
                match role {
                    $(Role::$name => &self.$role,)*
                }
            }
        }

        impl UiPalette<Role> {
            pub const ROLES: Self = UiPalette { $($role: Role::$name,)* };
        }
    };
}

roles!(
    canvas: Canvas,
    surface: Surface,
    surface_hover: SurfaceHover,
    surface_selected: SurfaceSelected,
    chrome: Chrome,
    elevated: Elevated,
    input: Input,
    text: Text,
    text_secondary: TextSecondary,
    text_muted: TextMuted,
    border: Border,
    border_subtle: BorderSubtle,
    accent: Accent,
    accent_foreground: AccentForeground,
    info: Info,
    positive: Positive,
    warning: Warning,
    destructive: Destructive,
    destructive_foreground: DestructiveForeground,
    scrim: Scrim,
);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Destructive,
    #[default]
    Ghost,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InteractionState {
    #[default]
    Rest,
    Hovered,
    Disabled,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ControlState {
    pub interaction: InteractionState,
    pub selected: bool,
}

impl ControlState {
    #[must_use]
    pub const fn new(interaction: InteractionState) -> Self {
        Self {
            interaction,
            selected: false,
        }
    }

    #[must_use]
    pub const fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectionState {
    #[default]
    None,
    Selected,
    Focused,
    Disabled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NoticeTone {
    Neutral,
    #[default]
    Info,
    Positive,
    Warning,
    Error,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModalSize {
    Compact,
    #[default]
    Medium,
    Wide,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticStyle<T = Rgba> {
    pub foreground: T,
    pub background: Option<T>,
    pub emphasized: bool,
}

impl<T> SemanticStyle<T> {
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> SemanticStyle<U> {
        SemanticStyle {
            foreground: f(self.foreground),
            background: self.background.map(f),
            emphasized: self.emphasized,
        }
    }
}

impl<T: Copy> UiPalette<T> {
    #[must_use]
    pub fn control_style(self, variant: ButtonVariant, state: ControlState) -> SemanticStyle<T> {
        if matches!(state.interaction, InteractionState::Disabled) {
            return SemanticStyle {
                foreground: self.text_muted,
                background: Some(self.surface_selected),
                emphasized: false,
            };
        }
        let (foreground, background) = match (variant, state.interaction, state.selected) {
            (ButtonVariant::Primary, _, _) => (self.accent_foreground, Some(self.accent)),
            (ButtonVariant::Destructive, _, _) => {
                (self.destructive_foreground, Some(self.destructive))
            }
            (ButtonVariant::Ghost, _, true) => (self.accent, Some(self.surface_selected)),
            (ButtonVariant::Secondary | ButtonVariant::Ghost, InteractionState::Hovered, _) => {
                (self.text, Some(self.surface_hover))
            }
            (ButtonVariant::Secondary, _, _) => (self.text, Some(self.surface)),
            (ButtonVariant::Ghost, _, false) => (self.text_muted, None),
        };
        SemanticStyle {
            foreground,
            background,
            emphasized: state.selected,
        }
    }
    #[must_use]
    pub fn selection_style(self, state: SelectionState) -> SemanticStyle<T> {
        match state {
            SelectionState::None => SemanticStyle {
                foreground: self.text,
                background: Some(self.surface),
                emphasized: false,
            },
            SelectionState::Selected => SemanticStyle {
                foreground: self.accent,
                background: Some(self.surface_selected),
                emphasized: false,
            },
            SelectionState::Focused => SemanticStyle {
                foreground: self.accent_foreground,
                background: Some(self.accent),
                emphasized: true,
            },
            SelectionState::Disabled => SemanticStyle {
                foreground: self.text_muted,
                background: Some(self.surface),
                emphasized: false,
            },
        }
    }
    #[must_use]
    pub fn notice_style(self, tone: NoticeTone) -> SemanticStyle<T> {
        let foreground = match tone {
            NoticeTone::Neutral => self.text_secondary,
            NoticeTone::Info => self.info,
            NoticeTone::Positive => self.positive,
            NoticeTone::Warning => self.warning,
            NoticeTone::Error => self.destructive,
        };
        SemanticStyle {
            foreground,
            background: None,
            emphasized: matches!(tone, NoticeTone::Warning | NoticeTone::Error),
        }
    }
}

impl UiPalette {
    #[must_use]
    pub const fn light() -> Self {
        Self {
            canvas: Rgba::rgb(0xf5f5f7),
            surface: Rgba::rgb(0xf5f5f7),
            surface_hover: Rgba::new(0, 0, 0, 13),
            surface_selected: Rgba::new(0, 0, 0, 26),
            chrome: Rgba::rgb(0xebebed),
            elevated: Rgba::rgb(0xffffff),
            input: Rgba::rgb(0xffffff),
            text: Rgba::rgb(0x1d1d1f),
            text_secondary: Rgba::rgb(0x424245),
            text_muted: Rgba::rgb(0x6e6e73),
            border: Rgba::rgb(0xd2d2d7),
            border_subtle: Rgba::rgb(0xe5e5ea),
            accent: Rgba::rgb(0x0a6cff),
            accent_foreground: Rgba::rgb(0xffffff),
            info: Rgba::rgb(0x0a6cff),
            positive: Rgba::rgb(0x248a3d),
            warning: Rgba::rgb(0xb25000),
            destructive: Rgba::rgb(0xd70015),
            destructive_foreground: Rgba::rgb(0xffffff),
            scrim: Rgba::new(0, 0, 0, 89),
        }
    }

    #[must_use]
    pub const fn dark() -> Self {
        Self {
            canvas: Rgba::rgb(0x1c1c1e),
            surface: Rgba::rgb(0x1c1c1e),
            surface_hover: Rgba::new(255, 255, 255, 18),
            surface_selected: Rgba::new(255, 255, 255, 33),
            chrome: Rgba::rgb(0x161618),
            elevated: Rgba::rgb(0x2c2c2e),
            input: Rgba::rgb(0x2c2c2e),
            text: Rgba::rgb(0xf5f5f7),
            text_secondary: Rgba::rgb(0xc7c7cc),
            text_muted: Rgba::rgb(0x98989d),
            border: Rgba::rgb(0x48484a),
            border_subtle: Rgba::rgb(0x38383a),
            accent: Rgba::rgb(0x0a84ff),
            accent_foreground: Rgba::rgb(0xffffff),
            info: Rgba::rgb(0x0a84ff),
            positive: Rgba::rgb(0x30d158),
            warning: Rgba::rgb(0xff9f0a),
            destructive: Rgba::rgb(0xff453a),
            destructive_foreground: Rgba::rgb(0xffffff),
            scrim: Rgba::new(0, 0, 0, 128),
        }
    }
}

impl Default for UiPalette {
    fn default() -> Self {
        Self::light()
    }
}
