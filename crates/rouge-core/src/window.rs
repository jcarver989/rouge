use crate::event::{Chord, Key, KeyEvent, Modifiers};
use crate::tree::edit::EditOp;
use crate::ui::Ui;
use rouge_reactive::{ReadSignal, Signal, expect_context};
use std::any::Any;
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decorations {
    #[default]
    Server,
    Client,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

#[derive(Clone)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl Menu {
    pub fn edit() -> Self {
        let mut items: Vec<_> = EditAction::ALL
            .iter()
            .map(|action| action.menu_item())
            .collect();
        items.insert(2, MenuItem::Separator);
        Menu::new("Edit", items)
    }

    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Menu {
            title: title.into(),
            items,
        }
    }
}

#[derive(Clone)]
pub enum MenuItem {
    Action {
        title: String,
        action: Rc<dyn Any>,
        shortcut: Option<Chord>,
    },
    Callback {
        title: String,
        on_select: Rc<dyn Fn()>,
        shortcut: Option<Chord>,
    },
    Separator,
    Submenu(Menu),
}

impl MenuItem {
    pub fn action<T: Any>(title: impl Into<String>, action: T) -> Self {
        MenuItem::Action {
            title: title.into(),
            action: Rc::new(action),
            shortcut: None,
        }
    }

    pub fn callback(title: impl Into<String>, on_select: impl Fn() + 'static) -> Self {
        MenuItem::Callback {
            title: title.into(),
            on_select: Rc::new(on_select),
            shortcut: None,
        }
    }

    pub fn activate(&self) {
        match self {
            MenuItem::Callback { on_select, .. } => on_select(),
            MenuItem::Action { .. } => window_handle().dispatch_menu_item(self.clone()),
            MenuItem::Separator | MenuItem::Submenu(_) => {}
        }
    }

    pub fn shortcut(mut self, chord: Chord) -> Self {
        if let MenuItem::Action { shortcut, .. } | MenuItem::Callback { shortcut, .. } = &mut self {
            *shortcut = Some(chord);
        }
        self
    }

    pub fn dispatch(&self, ui: &mut Ui) -> bool {
        match self {
            MenuItem::Action { action, .. } => ui.dispatch_action_ref(&**action),
            MenuItem::Callback { on_select, .. } => {
                ui.invoke(|| on_select());
                true
            }
            MenuItem::Separator | MenuItem::Submenu(_) => false,
        }
    }
}

macro_rules! edit_commands {
    ($($action:ident: $title:literal, $key:literal, $shift:literal;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum EditAction { $($action,)* }

        impl EditAction {
            const ALL: &[Self] = &[$(Self::$action,)*];

            fn menu_item(self) -> MenuItem {
                let (title, key, shift) = match self {
                    $(Self::$action => ($title, $key, $shift),)*
                };
                let mut modifiers = Modifiers::COMMAND;
                modifiers.shift = shift;
                MenuItem::action(title, self).shortcut(Chord::new(Key::character(key), modifiers))
            }

            pub(crate) fn from_key(event: &KeyEvent) -> Option<Self> {
                let Key::Character(key) = &event.key else { return None };
                if !event.modifiers.command() { return None; }
                match (key.as_str(), event.modifiers.shift) {
                    $(($key, shift) if $key != "z" || shift == $shift => Some(Self::$action),)*
                    ("y", _) => Some(Self::Redo),
                    _ => None,
                }
            }

            pub(crate) fn operation(self) -> EditOp {
                match self { $(Self::$action => EditOp::$action,)* }
            }
        }
    };
}

edit_commands! {
    Undo: "Undo", "z", false;
    Redo: "Redo", "z", true;
    Cut: "Cut", "x", false;
    Copy: "Copy", "c", false;
    Paste: "Paste", "v", false;
    SelectAll: "Select All", "a", false;
}

pub enum WindowCommand {
    SetTitle(String),
    Minimize,
    Zoom,
    Close,
    StartDrag,
    StartResize(ResizeEdge),
    Quit,
    Menu(MenuItem),
}

#[derive(Clone, Copy)]
pub struct WindowHandle {
    pub(crate) commands: Signal<UnboundedSender<WindowCommand>>,
    pub(crate) title: Signal<String>,
    pub(crate) appearance: Signal<Appearance>,
}

pub fn window_handle() -> WindowHandle {
    expect_context::<WindowHandle>()
}

pub fn appearance() -> ReadSignal<Appearance> {
    window_handle().appearance.read_only()
}

impl WindowHandle {
    fn send(self, command: WindowCommand) {
        if self.commands.is_alive() {
            self.commands.with_untracked(|commands| {
                let _ = commands.send(command);
            });
        }
    }

    pub fn title(self) -> ReadSignal<String> {
        self.title.read_only()
    }

    pub fn set_title(self, title: impl Into<String>) {
        let title = title.into();
        if !self.title.is_alive() {
            return;
        }
        self.title.set(title.clone());
        self.send(WindowCommand::SetTitle(title));
    }

    pub fn minimize(self) {
        self.send(WindowCommand::Minimize);
    }
    pub fn zoom(self) {
        self.send(WindowCommand::Zoom);
    }
    pub fn close(self) {
        self.send(WindowCommand::Close);
    }
    pub fn start_drag(self) {
        self.send(WindowCommand::StartDrag);
    }
    pub fn start_resize(self, edge: ResizeEdge) {
        self.send(WindowCommand::StartResize(edge));
    }
    pub fn quit(self) {
        self.send(WindowCommand::Quit);
    }
    pub fn dispatch_menu_item(self, item: MenuItem) {
        self.send(WindowCommand::Menu(item));
    }
}
