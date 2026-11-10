use crate::event::{Chord, Key, Modifiers};
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
        let action = |title, action, key| {
            MenuItem::action(title, action).shortcut(Chord::new(key, Modifiers::COMMAND))
        };
        Menu::new(
            "Edit",
            vec![
                action("Undo", EditAction::Undo, Key::character("z")),
                MenuItem::action("Redo", EditAction::Redo),
                MenuItem::Separator,
                action("Cut", EditAction::Cut, Key::character("x")),
                action("Copy", EditAction::Copy, Key::character("c")),
                action("Paste", EditAction::Paste, Key::character("v")),
                action("Select All", EditAction::SelectAll, Key::character("a")),
            ],
        )
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

    pub fn shortcut(mut self, chord: Chord) -> Self {
        if let MenuItem::Action { shortcut, .. } = &mut self {
            *shortcut = Some(chord);
        }
        self
    }

    pub fn dispatch(&self, ui: &mut Ui) -> bool {
        match self {
            MenuItem::Action { action, .. } => ui.dispatch_action_ref(&**action),
            MenuItem::Separator | MenuItem::Submenu(_) => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
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
