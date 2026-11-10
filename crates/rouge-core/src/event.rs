use rouge_render::Point;
use std::cell::Cell;
use std::str::FromStr;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Other(u16),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[expect(clippy::struct_excessive_bools)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers::new(false, false, false, false);
    pub const SHIFT: Modifiers = Modifiers::new(true, false, false, false);
    pub const CTRL: Modifiers = Modifiers::new(false, true, false, false);
    pub const ALT: Modifiers = Modifiers::new(false, false, true, false);
    pub const META: Modifiers = Modifiers::new(false, false, false, true);
    pub const COMMAND: Modifiers = if cfg!(target_os = "macos") {
        Modifiers::META
    } else {
        Modifiers::CTRL
    };

    #[expect(clippy::fn_params_excessive_bools)]
    const fn new(shift: bool, ctrl: bool, alt: bool, meta: bool) -> Modifiers {
        Modifiers {
            shift,
            ctrl,
            alt,
            meta,
        }
    }

    pub const fn with(self, other: Modifiers) -> Modifiers {
        Modifiers::new(
            self.shift || other.shift,
            self.ctrl || other.ctrl,
            self.alt || other.alt,
            self.meta || other.meta,
        )
    }

    pub fn command(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.meta
        } else {
            self.ctrl
        }
    }

    pub fn word(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.alt
        } else {
            self.ctrl
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Character(String),
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    Escape,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Other,
}

impl Key {
    pub fn character(text: &str) -> Key {
        match text {
            " " => Key::Space,
            text => Key::Character(text.to_lowercase()),
        }
    }
}

#[derive(Debug)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
    pub text: Option<String>,
    pub repeat: bool,
    pub(crate) stopped: Cell<bool>,
}

impl KeyEvent {
    pub fn new(key: Key) -> Self {
        KeyEvent {
            key,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
            stopped: Cell::new(false),
        }
    }

    pub fn typed(text: &str) -> Self {
        KeyEvent::new(Key::character(text)).with_text(text)
    }

    pub fn with_modifiers(mut self, modifiers: Modifiers) -> Self {
        self.modifiers = modifiers;
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn repeated(mut self) -> Self {
        self.repeat = true;
        self
    }

    pub fn stop_propagation(&self) {
        self.stopped.set(true);
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChordError {
    #[error("key chord is empty")]
    Empty,
    #[error("unknown key `{0}` in key chord")]
    UnknownKey(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl Chord {
    pub fn parse(value: &str) -> Result<Self, ChordError> {
        value.parse()
    }

    pub fn display(&self) -> String {
        let mac = cfg!(target_os = "macos");
        let mut parts = Vec::new();
        for (enabled, label) in [
            (self.modifiers.ctrl, if mac { "⌃" } else { "Ctrl" }),
            (self.modifiers.alt, if mac { "⌥" } else { "Alt" }),
            (self.modifiers.shift, if mac { "⇧" } else { "Shift" }),
            (self.modifiers.meta, if mac { "⌘" } else { "Super" }),
        ] {
            if enabled {
                parts.push(label.to_owned());
            }
        }
        parts.push(match &self.key {
            Key::Character(text) => text.to_uppercase(),
            Key::Enter => if mac { "↩" } else { "Enter" }.into(),
            Key::Tab => if mac { "⇥" } else { "Tab" }.into(),
            Key::Space => "Space".into(),
            Key::Backspace => if mac { "⌫" } else { "Backspace" }.into(),
            Key::Delete => if mac { "⌦" } else { "Delete" }.into(),
            Key::Escape => if mac { "⎋" } else { "Esc" }.into(),
            Key::ArrowLeft => "←".into(),
            Key::ArrowRight => "→".into(),
            Key::ArrowUp => "↑".into(),
            Key::ArrowDown => "↓".into(),
            Key::Home => "Home".into(),
            Key::End => "End".into(),
            Key::PageUp => "Page Up".into(),
            Key::PageDown => "Page Down".into(),
            Key::F(number) => format!("F{number}"),
            Key::Other => "?".into(),
        });
        parts.join(if mac { "" } else { "+" })
    }

    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Chord { key, modifiers }
    }

    pub fn matches(&self, event: &KeyEvent) -> bool {
        let typed = matches!(self.key, Key::Character(_));
        let shift = typed || self.modifiers.shift == event.modifiers.shift;
        let rest = |m: Modifiers| (m.ctrl, m.alt, m.meta);
        self.key == event.key && shift && rest(self.modifiers) == rest(event.modifiers)
    }
}

impl FromStr for Chord {
    type Err = ChordError;

    fn from_str(chord: &str) -> Result<Self, ChordError> {
        let mut modifiers = Modifiers::NONE;
        let mut key = None;
        for part in chord.split('-').map(str::trim) {
            let lower = part.to_lowercase();
            let named = match lower.as_str() {
                "cmd" | "command" | "secondary" => Err(Modifiers::COMMAND),
                "shift" => Err(Modifiers::SHIFT),
                "ctrl" | "control" => Err(Modifiers::CTRL),
                "alt" | "option" => Err(Modifiers::ALT),
                "meta" | "super" => Err(Modifiers::META),
                "enter" | "return" => Ok(Key::Enter),
                "tab" => Ok(Key::Tab),
                "space" => Ok(Key::Space),
                "backspace" => Ok(Key::Backspace),
                "delete" => Ok(Key::Delete),
                "escape" | "esc" => Ok(Key::Escape),
                "left" => Ok(Key::ArrowLeft),
                "right" => Ok(Key::ArrowRight),
                "up" => Ok(Key::ArrowUp),
                "down" => Ok(Key::ArrowDown),
                "home" => Ok(Key::Home),
                "end" => Ok(Key::End),
                "pageup" => Ok(Key::PageUp),
                "pagedown" => Ok(Key::PageDown),
                "" => Ok(Key::Character("-".into())),
                name => {
                    let function = name.strip_prefix('f').and_then(|n| n.parse().ok());
                    match function.filter(|n| (1..=24).contains(n)) {
                        Some(n) => Ok(Key::F(n)),
                        None if name.chars().count() == 1 => Ok(Key::Character(lower.clone())),
                        None => return Err(ChordError::UnknownKey(part.to_owned())),
                    }
                }
            };
            match named {
                Err(modifier) => modifiers = modifiers.with(modifier),
                Ok(named) => key = Some(named),
            }
        }
        key.map(|key| Chord { key, modifiers })
            .ok_or(ChordError::Empty)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusEvent {
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeEvent {
    Preedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    Commit(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cursor {
    #[default]
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    NotAllowed,
    Crosshair,
    ResizeHorizontal,
    ResizeVertical,
}

#[derive(Debug)]
pub struct PointerEvent {
    pub position: Point,
    pub button: PointerButton,
    pub modifiers: Modifiers,
    pub click_count: u32,
    pub(crate) local: Cell<Point>,
    pub(crate) stopped: Cell<bool>,
    pub(crate) captured: Cell<bool>,
}

impl PointerEvent {
    pub(crate) fn new(position: Point, button: PointerButton) -> Self {
        PointerEvent {
            position,
            button,
            modifiers: Modifiers::NONE,
            click_count: 1,
            local: Cell::new(position),
            stopped: Cell::new(false),
            captured: Cell::new(false),
        }
    }

    pub fn local(&self) -> Point {
        self.local.get()
    }

    pub fn capture(&self) {
        self.captured.set(true);
    }

    pub fn stop_propagation(&self) {
        self.stopped.set(true);
    }
}

#[derive(Debug)]
pub struct ScrollEvent {
    pub position: Point,
    pub delta: Point,
    pub(crate) stopped: Cell<bool>,
}

impl ScrollEvent {
    pub(crate) fn new(position: Point, delta: Point) -> Self {
        ScrollEvent {
            position,
            delta,
            stopped: Cell::new(false),
        }
    }

    pub fn stop_propagation(&self) {
        self.stopped.set(true);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DragPhase {
    Start,
    Move,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragEvent {
    pub phase: DragPhase,
    pub position: Point,
    pub start: Point,
    pub delta: Point,
    pub translation: Point,
    pub velocity: Point,
}
